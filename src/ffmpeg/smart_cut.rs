//! Smart-cut : découpe précise à la frame en minimisant le ré-encodage.
//!
//! Pour un segment `[start, end]` :
//! - **Head**  : `[start, K_first]` ré-encodé   (si start n'est pas aligné keyframe)
//! - **Middle**: `[K_first, K_last]` copié      (lossless, instantané)
//! - **Tail**  : `[K_last, end]` ré-encodé      (si end n'est pas aligné keyframe)
//!
//! Les fragments sont produits en MPEG-TS (qui supporte la concaténation par
//! flux sans conflit de SPS/PPS), puis assemblés en un seul fichier final via
//! le concat protocol de ffmpeg (`-i "concat:f1.ts|f2.ts|..."`).
//!
//! Si l'ensemble du segment ne contient aucune keyframe ou est trop court pour
//! avoir un Middle, on ré-encode le tout (rare en pratique).

use super::commands::concat_demuxer_line;
use super::keyframes::{first_keyframe_at_or_after, last_keyframe_at_or_before};
use super::paths::{apply_platform_flags_tokio, ffmpeg_path, install_hint};
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

/// Type de fragment dans un plan de smart-cut.
#[derive(Debug, Clone, PartialEq)]
pub enum FragmentKind {
    /// Portion à copier sans ré-encodage (`-c copy`)
    Copy,
    /// Portion à ré-encoder à la frame près
    Reencode,
}

/// Un fragment d'un segment, à produire séparément avant concat final.
#[derive(Debug, Clone, PartialEq)]
pub struct Fragment {
    pub start: f64,
    pub end: f64,
    pub kind: FragmentKind,
}

impl Fragment {
    pub fn duration(&self) -> f64 {
        (self.end - self.start).max(0.0)
    }
}

/// Calcule le plan de découpe smart-cut d'un segment.
///
/// `keyframes` doit être trié croissant. `tolerance` est l'écart max
/// (en secondes, typiquement 1 frame) sous lequel on considère une coupe
/// comme déjà alignée à une keyframe.
///
/// Retourne 1 à 3 fragments à exécuter dans l'ordre.
pub fn plan_smart_cut(
    start: f64,
    end: f64,
    keyframes: &[f64],
    tolerance: f64,
) -> Vec<Fragment> {
    debug_assert!(end > start, "end doit être > start");

    if keyframes.is_empty() {
        // Pas de carte de keyframes → tout ré-encoder pour garantir la précision
        return vec![Fragment {
            start,
            end,
            kind: FragmentKind::Reencode,
        }];
    }

    // K_first : 1ère keyframe >= start (alignée si à ε près)
    let k_first = first_keyframe_at_or_after(keyframes, start, tolerance);
    // K_last  : dernière keyframe <= end (alignée si à ε près)
    let k_last = last_keyframe_at_or_before(keyframes, end, tolerance);

    // Cas dégénérés : pas de keyframe utilisable dans [start, end]
    let (k_first, k_last) = match (k_first, k_last) {
        (Some(kf), Some(kl)) if kl > kf + tolerance => (kf, kl),
        _ => {
            // Aucune zone "Middle" possible → on ré-encode tout le segment
            return vec![Fragment {
                start,
                end,
                kind: FragmentKind::Reencode,
            }];
        }
    };

    // Snap : si start (resp. end) est dans la tolérance d'une keyframe,
    // on aligne directement et on saute le head/tail.
    let start_aligned = (k_first - start).abs() <= tolerance;
    let end_aligned = (k_last - end).abs() <= tolerance;

    let mut fragments = Vec::with_capacity(3);

    if !start_aligned {
        fragments.push(Fragment {
            start,
            end: k_first,
            kind: FragmentKind::Reencode,
        });
    }

    fragments.push(Fragment {
        start: k_first,
        end: k_last,
        kind: FragmentKind::Copy,
    });

    if !end_aligned {
        fragments.push(Fragment {
            start: k_last,
            end,
            kind: FragmentKind::Reencode,
        });
    }

    fragments
}

// ===========================================================================
// Pipeline d'exécution : produit les fragments en MPEG-TS puis concat.
// ===========================================================================

/// Codec vidéo de la source — détermine le bitstream filter à appliquer.
#[derive(Debug, Clone, Copy)]
pub enum SourceVideoCodec {
    H264,
    H265,
    /// Tout autre codec : on désactive les bitstream filters spécifiques
    /// et on accepte que la concat puisse échouer si ré-encodage hétérogène.
    Other,
}

impl SourceVideoCodec {
    pub fn from_codec_name(name: Option<&str>) -> Self {
        match name {
            Some("h264") => Self::H264,
            Some("hevc") | Some("h265") => Self::H265,
            _ => Self::Other,
        }
    }

    /// Bitstream filter à appliquer côté vidéo pour produire un flux MPEG-TS sain.
    fn ts_video_bsf(self) -> Option<&'static str> {
        match self {
            Self::H264 => Some("h264_mp4toannexb"),
            Self::H265 => Some("hevc_mp4toannexb"),
            Self::Other => None,
        }
    }

    /// Encodeur libavcodec à utiliser pour le ré-encodage des fragments Head/Tail.
    fn reencode_codec(self) -> &'static str {
        match self {
            Self::H264 => "libx264",
            Self::H265 => "libx265",
            // Pour les codecs inconnus, fallback sur libx264
            Self::Other => "libx264",
        }
    }
}

/// Exécute un plan de smart-cut : produit chaque fragment dans `temp_dir`,
/// concat le tout dans `output`, puis nettoie les fichiers temporaires.
pub async fn execute_smart_cut(
    input: &Path,
    output: &Path,
    fragments: &[Fragment],
    codec: SourceVideoCodec,
) -> Result<()> {
    if fragments.is_empty() {
        return Err(anyhow!("Smart-cut: plan vide"));
    }

    // Dossier temp dédié à ce job, à côté du fichier de sortie pour rester
    // sur le même volume (évite des copies inter-disques au remux final).
    let stem = output
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "smartcut".to_string());
    let temp_dir = output
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(".{}_smartcut_tmp", stem));

    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| anyhow!("Cannot create smart-cut temp dir: {}", e))?;

    // Cleanup garanti même en cas d'erreur via un guard
    let result = run_smart_cut_inner(input, output, fragments, codec, &temp_dir).await;

    // Best-effort cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);

    result
}

async fn run_smart_cut_inner(
    input: &Path,
    output: &Path,
    fragments: &[Fragment],
    codec: SourceVideoCodec,
    temp_dir: &Path,
) -> Result<()> {
    // 1) Générer les fragments .ts en série (séquentiel pour ne pas saturer le disque/CPU)
    let mut ts_paths: Vec<PathBuf> = Vec::with_capacity(fragments.len());
    for (i, frag) in fragments.iter().enumerate() {
        let ts_path = temp_dir.join(format!("frag_{:03}.ts", i));
        produce_fragment_ts(input, &ts_path, frag, codec).await?;
        ts_paths.push(ts_path);
    }

    // 2) Concat demuxer (plus fiable que le concat protocol pour les transitions
    // entre fragments hétérogènes copy/reencode).
    concat_ts_fragments(&ts_paths, output, codec, temp_dir).await
}

/// Produit un fragment unique au format MPEG-TS.
async fn produce_fragment_ts(
    input: &Path,
    output: &Path,
    fragment: &Fragment,
    codec: SourceVideoCodec,
) -> Result<()> {
    let duration = fragment.duration();
    if duration <= 0.0 {
        return Err(anyhow!(
            "Fragment de durée nulle ({}..{})",
            fragment.start,
            fragment.end
        ));
    }

    let mut args: Vec<String> = Vec::new();
    args.push("-y".into());

    // -ss avant -i : seek rapide ET précis (ffmpeg ≥ 4 fait un seek décodé après cette position).
    args.push("-ss".into());
    args.push(format!("{:.3}", fragment.start));
    args.push("-i".into());
    args.push(input.to_string_lossy().into_owned());
    args.push("-t".into());
    args.push(format!("{:.3}", duration));

    match fragment.kind {
        FragmentKind::Copy => {
            // Copy lossless : aucun ré-encodage.
            args.extend([
                "-c".to_string(),
                "copy".to_string(),
                "-avoid_negative_ts".to_string(),
                "make_zero".to_string(),
            ]);
        }
        FragmentKind::Reencode => {
            // Ré-encodage des bouts (typiquement < 1-2s à transcoder).
            //
            // Choix critiques pour éviter une dernière frame noire :
            //   -bf 0          : pas de B-frames → la dernière frame encodée
            //                    est la dernière frame écrite, sans dépendre
            //                    d'une frame future qui n'existerait pas.
            //   -fps_mode passthrough : ne pas dupliquer ni jeter de frames,
            //                    on garde exactement les frames source.
            //   -g 1           : keyframe à chaque frame du fragment réencodé
            //                    (sur 1-2s, l'overhead taille est négligeable
            //                    et ça simplifie la concat avec le Middle copy).
            args.extend([
                "-threads".to_string(),
                "0".to_string(),
                "-c:v".to_string(),
                codec.reencode_codec().to_string(),
                "-preset".to_string(),
                "veryfast".to_string(),
                "-crf".to_string(),
                "16".to_string(),
                "-pix_fmt".to_string(),
                "yuv420p".to_string(),
                "-bf".to_string(),
                "0".to_string(),
                "-g".to_string(),
                "1".to_string(),
                "-fps_mode".to_string(),
                "passthrough".to_string(),
                "-c:a".to_string(),
                "aac".to_string(),
                "-b:a".to_string(),
                "192k".to_string(),
                // Empêche l'audio AAC (frame de 23ms à 44.1kHz) de "tirer"
                // une frame vidéo de pad noire pour aligner les durées.
                "-shortest".to_string(),
            ]);
        }
    }

    // Bitstream filter vidéo pour MPEG-TS (annexb)
    if let Some(bsf) = codec.ts_video_bsf() {
        args.push("-bsf:v".into());
        args.push(bsf.into());
    }

    args.push("-f".into());
    args.push("mpegts".into());
    args.push(output.to_string_lossy().into_owned());

    run_ffmpeg(&args).await.map_err(|e| {
        anyhow!(
            "Smart-cut fragment {} ({}s..{}s, {:?}) failed: {}",
            output.file_name().unwrap_or_default().to_string_lossy(),
            fragment.start,
            fragment.end,
            fragment.kind,
            e
        )
    })
}

/// Concatène une liste de fragments .ts en un seul fichier final via le
/// **concat demuxer** (plus fiable que le concat protocol pour les transitions
/// entre fragments copy/reencode aux paramètres légèrement différents).
async fn concat_ts_fragments(
    ts_paths: &[PathBuf],
    output: &Path,
    codec: SourceVideoCodec,
    temp_dir: &Path,
) -> Result<()> {
    if ts_paths.is_empty() {
        return Err(anyhow!("Aucun fragment à concaténer"));
    }

    // Crée le fichier liste pour le concat demuxer.
    let list_path = temp_dir.join("concat_list.txt");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&list_path)
            .map_err(|e| anyhow!("Cannot create concat list: {}", e))?;
        for ts_path in ts_paths {
            writeln!(f, "{}", concat_demuxer_line(ts_path))
                .map_err(|e| anyhow!("Cannot write concat list: {}", e))?;
        }
    }

    let mut args: Vec<String> = vec![
        "-y".into(),
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        list_path.to_string_lossy().into_owned(),
        "-c".into(),
        "copy".into(),
        "-avoid_negative_ts".into(),
        "make_zero".into(),
    ];

    // Sortie en MP4 ? Il faut convertir AAC ADTS → ASC.
    if let Some(ext) = output.extension().and_then(|e| e.to_str()) {
        let is_mp4_like = matches!(ext.to_ascii_lowercase().as_str(), "mp4" | "mov" | "m4v");
        if is_mp4_like {
            args.push("-bsf:a".into());
            args.push("aac_adtstoasc".into());
        }
    }

    let _ = codec; // codec déjà encodé dans les fragments .ts, plus rien à faire ici

    args.push(output.to_string_lossy().into_owned());

    run_ffmpeg(&args)
        .await
        .map_err(|e| anyhow!("Smart-cut concat failed: {}", e))
}

/// Helper : lance ffmpeg avec les args, capture stderr, retourne erreur détaillée.
async fn run_ffmpeg(args: &[String]) -> Result<()> {
    let mut cmd = Command::new(ffmpeg_path());
    cmd.args(args).stdout(Stdio::null()).stderr(Stdio::piped());
    apply_platform_flags_tokio(&mut cmd);

    let output = cmd
        .output()
        .await
        .map_err(|e| anyhow!("Impossible de lancer FFmpeg: {}. {}", e, install_hint()))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Ne garde que les lignes pertinentes pour ne pas spammer
        let interesting: Vec<&str> = stderr
            .lines()
            .filter(|l| {
                l.contains("Error")
                    || l.contains("error")
                    || l.contains("Invalid")
                    || l.contains("failed")
            })
            .take(5)
            .collect();
        let detail = if interesting.is_empty() {
            format!("status: {}", output.status)
        } else {
            interesting.join(" | ")
        };
        Err(anyhow!("FFmpeg: {}", detail))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: f64 = 0.04; // ~1 frame à 25fps

    #[test]
    fn cut_aligned_on_both_ends_is_pure_copy() {
        // Keyframes toutes les 2s, segment [2.0, 6.0] → tout en copy
        let kf = vec![0.0, 2.0, 4.0, 6.0, 8.0];
        let plan = plan_smart_cut(2.0, 6.0, &kf, TOL);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].kind, FragmentKind::Copy);
        assert!((plan[0].start - 2.0).abs() < 0.001);
        assert!((plan[0].end - 6.0).abs() < 0.001);
    }

    #[test]
    fn cut_misaligned_start_only_produces_head_plus_copy() {
        let kf = vec![0.0, 2.0, 4.0, 6.0];
        let plan = plan_smart_cut(1.5, 6.0, &kf, TOL);
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].kind, FragmentKind::Reencode);
        assert!((plan[0].start - 1.5).abs() < 0.001);
        assert!((plan[0].end - 2.0).abs() < 0.001);
        assert_eq!(plan[1].kind, FragmentKind::Copy);
        assert!((plan[1].start - 2.0).abs() < 0.001);
        assert!((plan[1].end - 6.0).abs() < 0.001);
    }

    #[test]
    fn cut_misaligned_end_only_produces_copy_plus_tail() {
        let kf = vec![0.0, 2.0, 4.0, 6.0];
        let plan = plan_smart_cut(2.0, 5.5, &kf, TOL);
        assert_eq!(plan.len(), 2);
        assert_eq!(plan[0].kind, FragmentKind::Copy);
        assert_eq!(plan[1].kind, FragmentKind::Reencode);
        assert!((plan[1].start - 4.0).abs() < 0.001);
        assert!((plan[1].end - 5.5).abs() < 0.001);
    }

    #[test]
    fn cut_misaligned_both_ends_produces_three_fragments() {
        let kf = vec![0.0, 2.0, 4.0, 6.0, 8.0];
        let plan = plan_smart_cut(1.3, 7.7, &kf, TOL);
        assert_eq!(plan.len(), 3);
        assert_eq!(plan[0].kind, FragmentKind::Reencode);
        assert_eq!(plan[1].kind, FragmentKind::Copy);
        assert_eq!(plan[2].kind, FragmentKind::Reencode);
        // continuité
        assert!((plan[0].end - plan[1].start).abs() < 0.001);
        assert!((plan[1].end - plan[2].start).abs() < 0.001);
    }

    #[test]
    fn empty_keyframes_falls_back_to_full_reencode() {
        let plan = plan_smart_cut(1.0, 5.0, &[], TOL);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].kind, FragmentKind::Reencode);
    }

    #[test]
    fn no_middle_possible_falls_back_to_full_reencode() {
        // Segment dans un seul GOP : pas de Middle
        let kf = vec![0.0, 10.0];
        let plan = plan_smart_cut(2.0, 5.0, &kf, TOL);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].kind, FragmentKind::Reencode);
    }

    #[test]
    fn near_keyframe_within_tolerance_snaps() {
        let kf = vec![0.0, 2.0, 4.0, 6.0];
        // start=2.02 et end=4.01 sont à <0.04 d'une keyframe → snap, copy pur
        let plan = plan_smart_cut(2.02, 4.01, &kf, TOL);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].kind, FragmentKind::Copy);
    }

    #[test]
    fn fragments_cover_full_range_continuously() {
        let kf = vec![0.0, 2.0, 4.0, 6.0];
        let plan = plan_smart_cut(0.7, 5.3, &kf, TOL);
        assert!((plan.first().unwrap().start - 0.7).abs() < 0.001);
        assert!((plan.last().unwrap().end - 5.3).abs() < 0.001);
        for w in plan.windows(2) {
            assert!((w[0].end - w[1].start).abs() < 0.001);
        }
    }
}
