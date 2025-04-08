use std::{
    fs,
    process::{Command, Stdio},
    thread,
};

use anyhow::Context;
use camino::Utf8PathBuf;
use cargo_metadata::{CrateType, Message};
use clap::Parser;

#[derive(Parser, Clone, Copy)]
enum Action {
    #[command(about = "Build dlls")]
    BuildDll,
}

fn main() -> anyhow::Result<()> {
    match Action::parse() {
        Action::BuildDll => build_dlls()?,
    }

    Ok(())
}

fn build_dlls() -> anyhow::Result<()> {
    fn build_dll(target: &str) -> Option<Utf8PathBuf> {
        let mut command = Command::new("cargo")
            .args([
                "build",
                "--release",
                "-p",
                "asdf-overlay",
                "--message-format=json-render-diagnostics",
                &format!("--target={target}"),
            ])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let mut dll = None;

        let reader = std::io::BufReader::new(command.stdout.take().unwrap());
        for message in cargo_metadata::Message::parse_stream(reader) {
            if let Message::CompilerArtifact(artifact) = message.unwrap() {
                if artifact.target.name != "asdf_overlay"
                    || !artifact.target.crate_types.contains(&CrateType::CDyLib)
                {
                    continue;
                }

                if dll.is_none() {
                    dll = artifact.filenames.first().cloned();
                }
            }
        }

        dll
    }

    let tasks = thread::scope(|scope| {
        let x64_task = scope.spawn(|| build_dll("x86_64-pc-windows-msvc"));
        let x86_task = scope.spawn(|| build_dll("i686-pc-windows-msvc"));
        // let aarch64_task = scope.spawn(|| build_dll("aarch64-pc-windows-msvc"));

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
    // let aarch64_path = tasks
    //     .2
    //     .expect("aarch64 target build failed")
    //     .context("aarch64 build has no output")?;

    fs::copy(x64_path, "./asdf_overlay-x64.dll")?;
    fs::copy(x86_path, "./asdf_overlay-x86.dll")?;
    // fs::copy(aarch64_path, "./asdf_overlay-aarch64.dll")?;

    Ok(())
}
