use std::{
    env,
    ffi::OsStr,
    fs::{self, create_dir_all},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::Context;
use camino::{Utf8Path, Utf8PathBuf};
use cargo_metadata::Message;
use clap::Parser;

#[derive(Parser, Clone)]
enum Action {
    #[command(about = "Build overlay dlls")]
    BuildDll {
        /// Artifact output directory
        #[arg(short, long)]
        out: Option<PathBuf>,

        /// Addtional cargo arguments
        #[arg(last(true))]
        cargo_args: Vec<String>,
    },
    #[command(about = "Build node natives")]
    BuildNode {
        /// Artifact output directory
        #[arg(short, long)]
        out: Option<PathBuf>,

        /// Addtional cargo arguments
        #[arg(last(true))]
        cargo_args: Vec<String>,
    },
}

fn main() -> anyhow::Result<()> {
    match Action::parse() {
        Action::BuildDll { out, cargo_args } => {
            build_dlls(out.as_deref().unwrap_or(Path::new(".")), &cargo_args)?
        }
        Action::BuildNode { out, cargo_args } => {
            build_node(out.as_deref().unwrap_or(Path::new(".")), &cargo_args)?
        }
    }

    Ok(())
}

fn build_node(dir: &Path, cargo_args: &[String]) -> anyhow::Result<()> {
    create_dir_all(dir)?;
    let [x64_path, aarch64_path] = cargo_artifacts(
        cargo_args,
        "asdf-overlay-node",
        ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc"],
    );
    let x64_path = x64_path.context("x86_64 build has no output")?;
    let aarch64_path = aarch64_path.context("aarch64 build has no output")?;

    fs::copy(x64_path, dir.join("addon-x64.node"))?;
    fs::copy(aarch64_path, dir.join("addon-aarch64.node"))?;

    Ok(())
}

fn build_dlls(dir: &Path, cargo_args: &[String]) -> anyhow::Result<()> {
    create_dir_all(dir)?;
    let [x64_path, x86_path, aarch64_path] = cargo_artifacts(
        cargo_args,
        "asdf-overlay-dll",
        [
            "x86_64-pc-windows-msvc",
            "i686-pc-windows-msvc",
            "aarch64-pc-windows-msvc",
        ],
    );
    let x64_path = x64_path.context("x86_64 build has no output")?;
    let x86_path = x86_path.context("i686 build has no output")?;
    let aarch64_path = aarch64_path.context("aarch64 build has no output")?;

    fs::copy(x64_path, dir.join("asdf_overlay-x64.dll"))?;
    fs::copy(x86_path, dir.join("asdf_overlay-x86.dll"))?;
    fs::copy(aarch64_path, dir.join("asdf_overlay-aarch64.dll"))?;

    Ok(())
}

fn cargo_artifacts<const TARGETS: usize>(
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
    project: &str,
    targets: [&str; TARGETS],
) -> [Option<Utf8PathBuf>; TARGETS] {
    let mut command = Command::new(
        env::var_os("CARGO")
            .as_deref()
            .unwrap_or(OsStr::new("cargo")),
    )
    .arg("build")
    .args(args)
    .args(["-p", project, "--message-format=json-render-diagnostics"])
    .args(targets.iter().map(|target| format!("--target={target}")))
    .stdout(Stdio::piped())
    .spawn()
    .unwrap();

    let mut exe = [const { None }; TARGETS];

    let target_name = project.replace("-", "_");

    let reader = std::io::BufReader::new(command.stdout.take().unwrap());
    for message in cargo_metadata::Message::parse_stream(reader) {
        if let Message::CompilerArtifact(artifact) = message.unwrap() {
            if artifact.target.name != target_name {
                continue;
            }

            if let Some(name) = artifact.filenames.first() {
                let Some(target_path) = name.parent().and_then(Utf8Path::parent) else {
                    continue;
                };

                for (i, slot) in exe.iter_mut().enumerate() {
                    if target_path.ends_with(targets[i]) {
                        *slot = Some(name.clone());
                        break;
                    }
                }
            }
        }
    }
    command.wait().expect("cargo process exited unexpectedly");

    exe
}
