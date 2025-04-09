use std::{
    fs,
    process::{Command, Stdio},
    thread,
};

use anyhow::Context;
use camino::Utf8PathBuf;
use cargo_metadata::Message;
use clap::Parser;

#[derive(Parser, Clone, Copy)]
enum Action {
    #[command(about = "Build overlay dlls")]
    BuildDll,
    #[command(about = "Build node natives")]
    BuildNode,
}

fn main() -> anyhow::Result<()> {
    match Action::parse() {
        Action::BuildDll => build_dlls()?,
        Action::BuildNode => build_node()?,
    }

    Ok(())
}

fn build_node() -> anyhow::Result<()> {
    fn build_dll(target: &str) -> Option<Utf8PathBuf> {
        cargo_artifacts("asdf-overlay-node", target)
    }

    let tasks = thread::scope(|scope| {
        let x64_task = scope.spawn(|| build_dll("x86_64-pc-windows-msvc"));
        let aarch64_task = scope.spawn(|| build_dll("aarch64-pc-windows-msvc"));

        (x64_task.join(), aarch64_task.join())
    });
    let x64_path = tasks
        .0
        .expect("x86_64 target build failed")
        .context("x86_64 build has no output")?;
    let aarch64_path = tasks
        .1
        .expect("aarch64 target build failed")
        .context("aarch64 build has no output")?;

    fs::copy(x64_path, "./addon-x64.node")?;
    fs::copy(aarch64_path, "./addon-aarch64.node")?;

    Ok(())
}

fn build_dlls() -> anyhow::Result<()> {
    fn build_dll(target: &str) -> Option<Utf8PathBuf> {
        cargo_artifacts("asdf_overlay", target)
    }

    let tasks = thread::scope(|scope| {
        let x64_task = scope.spawn(|| build_dll("x86_64-pc-windows-msvc"));
        let x86_task = scope.spawn(|| build_dll("i686-pc-windows-msvc"));
        let aarch64_task = scope.spawn(|| build_dll("aarch64-pc-windows-msvc"));

        (x64_task.join(), x86_task.join(), aarch64_task.join())
    });
    let x64_path = tasks
        .0
        .expect("x86_64 target build failed")
        .context("x86_64 build has no output")?;
    let x86_path = tasks
        .1
        .expect("i686 target build failed")
        .context("i686 build has no output")?;
    let aarch64_path = tasks
        .2
        .expect("aarch64 target build failed")
        .context("aarch64 build has no output")?;

    fs::copy(x64_path, "./asdf_overlay-x64.dll")?;
    fs::copy(x86_path, "./asdf_overlay-x86.dll")?;
    fs::copy(aarch64_path, "./asdf_overlay-aarch64.dll")?;

    Ok(())
}

fn cargo_artifacts(project: &str, target: &str) -> Option<Utf8PathBuf> {
    let mut command = Command::new("cargo")
        .args([
            "build",
            "--release",
            "-p",
            project,
            "--message-format=json-render-diagnostics",
            &format!("--target={target}"),
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    let mut exe = None;

    let target_name = project.replace("-", "_");

    let reader = std::io::BufReader::new(command.stdout.take().unwrap());
    for message in cargo_metadata::Message::parse_stream(reader) {
        if let Message::CompilerArtifact(artifact) = message.unwrap() {
            if artifact.target.name != target_name {
                continue;
            }

            if exe.is_none() {
                exe = artifact.filenames.first().cloned();
            }
        }
    }
    command.wait().expect("cargo process exited unexpectedly");

    exe
}
