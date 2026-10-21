//! Image import: conversion to TIFF through the bundled Bio-Formats converter
//! (`lof2tiff.m`), the 1/20 downsampled registration image (`bg_down_img`) and image
//! metadata (`set_metadata`).

use crate::core::imgproc::stats::imadjust_u16;
use crate::core::tiff::{self, TiffFile};
use crate::core::workspace::DOWNSAMPLE;
use crate::core::Cancel;
use anyhow::{anyhow, bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Metadata {
    pub size: [usize; 2],
    pub down_size: [usize; 2],
    pub channel_count: usize,
    pub pixel_dims: Option<[f64; 2]>,
}

/// Where to find `java` and the converter jar.
#[derive(Clone, Debug)]
pub struct Converter {
    pub java: PathBuf,
    pub jar: PathBuf,
    /// Maximum JVM heap, e.g. `8g`.
    pub heap: String,
}

impl Converter {
    pub fn check_java(&self) -> Result<String> {
        let out = Command::new(&self.java)
            .arg("-version")
            .output()
            .with_context(|| format!("could not run '{}' — is Java installed?", self.java.display()))?;
        let text = String::from_utf8_lossy(&out.stderr);
        Ok(text.lines().next().unwrap_or("java").to_string())
    }

    /// Runs the converter, reporting progress as output size / input size.
    fn run_java(&self, src: &Path, dst: &Path, cancel: &Cancel, progress: &dyn Fn(f32)) -> Result<()> {
        if !self.jar.is_file() {
            bail!("converter not found at {}", self.jar.display());
        }
        let total = std::fs::metadata(src)?.len().max(1) as f32;
        std::fs::remove_file(dst).ok();
        let mut child = Command::new(&self.java)
            .arg(format!("-Xmx{}", self.heap))
            .arg("-jar")
            .arg(&self.jar)
            .arg(src)
            .arg(dst)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("could not start '{}' — is Java installed?", self.java.display()))?;
        // Drain the pipes so the converter never blocks on a full buffer.
        let drain = |r: Option<Box<dyn std::io::Read + Send>>| {
            std::thread::spawn(move || {
                let mut s = String::new();
                if let Some(mut r) = r {
                    r.read_to_string(&mut s).ok();
                }
                s
            })
        };
        let out = drain(child.stdout.take().map(|r| Box::new(r) as _));
        let err = drain(child.stderr.take().map(|r| Box::new(r) as _));
        let status = loop {
            if cancel.is_cancelled() {
                child.kill().ok();
                child.wait().ok();
                std::fs::remove_file(dst).ok();
                return Err(crate::core::Cancelled.into());
            }
            if let Some(s) = child.try_wait()? {
                break s;
            }
            if let Ok(m) = std::fs::metadata(dst) {
                progress((m.len() as f32 / total * 100.0).min(99.0));
            }
            std::thread::sleep(Duration::from_millis(500));
        };
        let (out, err) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
        for line in out.lines().chain(err.lines()).filter(|l| !l.trim().is_empty()) {
            log::debug!("[bioformats] {line}");
        }
        let failed = !status.success() || TiffFile::open(dst).is_err();
        if failed {
            let tail: Vec<&str> = err.lines().chain(out.lines()).filter(|l| !l.trim().is_empty()).collect();
            let tail = tail[tail.len().saturating_sub(5)..].join("\n");
            bail!("conversion failed for {}\n{tail}", src.display());
        }
        Ok(())
    }

    /// Produces the workspace TIFF for `src` (`bg_conv_img`). TIFFs are copied when they
    /// can be read directly; everything else goes through Bio-Formats.
    pub fn convert(&self, src: &Path, dst: &Path, cancel: &Cancel, progress: &dyn Fn(f32)) -> Result<()> {
        if let Some(dir) = dst.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if matches!(ext.as_str(), "tif" | "tiff") {
            match TiffFile::open(src).and_then(|t| t.pages.iter().try_for_each(|p| p.supported())) {
                Ok(()) => {
                    std::fs::copy(src, dst).with_context(|| format!("copying {}", src.display()))?;
                    return Ok(());
                }
                Err(e) => log::info!("{}: {e}; converting with Bio-Formats", src.display()),
            }
        }
        self.run_java(src, dst, cancel, progress)
    }
}

/// Writes the downsampled image: every channel subsampled ×20 and contrast-adjusted,
/// one page per channel. Unlike the MATLAB version all channels are kept, so any
/// channel can serve for registration.
pub fn downsample(conv: &Path, down: &Path) -> Result<()> {
    let tf = TiffFile::open(conv)?;
    let p0 = &tf.pages[0];
    let channels: Vec<_> = if p0.spp == 1 {
        (0..tf.pages.len())
            .map(|i| Ok(tf.read_subsampled(i, DOWNSAMPLE)?.swap_remove(0)))
            .collect::<Result<_>>()?
    } else {
        tf.read_subsampled(0, DOWNSAMPLE)?
    };
    let adjusted: Vec<_> = channels.iter().map(imadjust_u16).collect();
    let pages: Vec<Vec<_>> = adjusted.iter().map(|c| vec![c]).collect();
    tiff::write_pages(down, &pages)
}

pub fn read_metadata(conv: &Path, down: &Path) -> Result<Metadata> {
    let tf = TiffFile::open(conv)?;
    let dn = TiffFile::open(down)?;
    let p = &tf.pages[0];
    Ok(Metadata {
        size: [p.height, p.width],
        down_size: [dn.pages[0].height, dn.pages[0].width],
        channel_count: tf.channel_count(),
        pixel_dims: tf.pixel_size_um(),
    })
}

/// Full import of one image.
pub fn import(conv: &Converter, src: &Path, conv_path: &Path, down_path: &Path, cancel: &Cancel, progress: &dyn Fn(f32)) -> Result<Metadata> {
    conv.convert(src, conv_path, cancel, progress)?;
    cancel.check()?;
    progress(99.0);
    downsample(conv_path, down_path).context("downsampling")?;
    read_metadata(conv_path, down_path)
}

/// Locates `java`: `JAVA_HOME`, then `PATH`.
pub fn find_java() -> PathBuf {
    if let Ok(home) = std::env::var("JAVA_HOME") {
        let p = Path::new(&home).join("bin").join(if cfg!(windows) { "java.exe" } else { "java" });
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from("java")
}

pub fn not_found(what: &str) -> anyhow::Error {
    anyhow!("{what} not found")
}
