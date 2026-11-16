/*
 * Copyright (c) 2026. DisorderlyTeam
 */
use clap::{Parser, ValueEnum};
use std::fmt::Display;
use std::fs::{copy, create_dir_all, remove_dir_all, remove_file, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Platform {
    Linux,
    Windows,
}

impl Platform {
    fn current() -> Self {
        if cfg!(target_os = "linux") {
            Platform::Linux
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            unimplemented!()
        }
    }
}

impl Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Platform::Linux => write!(f, "Linux"),
            Platform::Windows => write!(f, "Windows"),
        }
    }
}

#[derive(Parser, Debug)]
struct Cli {
    #[clap(short, long)]
    platform: Option<Platform>,

    #[clap(short, long)]
    output_path: Option<PathBuf>,

    #[clap(short, long)]
    version: Option<String>,

    #[clap(short('y'))]
    confirm_all: bool,
}

fn input_bool(prompt: &str) -> bool {
    loop {
        println!("{prompt} [y/n]: ");
        std::io::stdout().flush().unwrap();
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        match input.trim().to_lowercase().as_str() {
            "y" => return true,
            "n" => return false,
            _ => continue,
        }
    }
}

fn main() {
    let cli = Cli::parse();

    let platform = cli.platform.unwrap_or_else(Platform::current);

    println!("Exporting for {platform}.");

    let triple = match platform {
        Platform::Linux => "x86_64-unknown-linux-gnu",
        Platform::Windows => "x86_64-pc-windows-gnu",
    };

    // Ensure export directory exists
    let export_dir = cli.output_path.unwrap_or_else(|| {
        let mut path = format!("./export/{platform}");
        path.into()
    });

    let dir_not_empty = export_dir.exists() && !export_dir.read_dir().unwrap().next().is_none();
    if dir_not_empty {
        println!("Export directory already exists and is not empty.");
        let remove_allowed = cli.confirm_all
            || input_bool(&format!(
                "Remove existing export directory ({})?",
                export_dir.display()
            ));
        if !remove_allowed {
            panic!("Aborting export.");
        }
        remove_dir_all(&export_dir).unwrap();
    }

    create_dir_all(&export_dir).unwrap();

    // println!("Ensuring target {triple} is available...");
    // let target_status = Command::new("rustup")
    //     .arg("target")
    //     .arg("add")
    //     .arg(triple)
    //     .status()
    //     .expect("Failed to add target");
    //
    // assert!(target_status.success());

    let is_cross_compile = platform != Platform::current();
    let compiling_text = if is_cross_compile {
        "Cross-compiling"
    } else {
        "Compiling"
    };
    println!("{compiling_text} project for {triple}...");
    let build_status = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .arg("--target")
        .arg(triple)
        .status()
        .expect("Failed to compile project");

    if !build_status.success() {
        if is_cross_compile && platform == Platform::Windows {
            panic!(
                "Failed to compile project. Please ensure you have installed the correct cross-compiler toolchain (mingw-w64-gcc)."
            );
        }
        panic!("Failed to compile project");
    }

    println!("Copying executable...");
    let exe_name = if platform == Platform::Windows {
        "game.exe"
    } else {
        "game"
    };
    copy(
        format!("./target/{triple}/release/{exe_name}"),
        export_dir.join(exe_name),
    )
    .unwrap();

    println!("Copying assets...");

    let walk = ignore::WalkBuilder::new("./assets")
        .add_custom_ignore_filename(".exportignore")
        .build();

    let export_assets_dir = export_dir.join("assets");

    for entry in walk {
        let entry = entry.unwrap();
        let absolute = entry.path();
        let relative = absolute.strip_prefix("./assets").unwrap();

        if entry.path().is_dir() {
            create_dir_all(export_assets_dir.join(relative)).unwrap();
        } else {
            println!("Copying {}...", relative.display());
            copy(absolute, export_assets_dir.join(relative)).unwrap();
        }
    }

    println!(
        "Exported to {export_dir}",
        export_dir = export_dir.display()
    );
    println!("The executable is named `{exe_name}`.");

    let architecture = triple.split('-').next().unwrap();
    let version = cli
        .version
        .map_or_else(|| "".to_string(), |v| format!("_v{v}"));
    let zip_name = format!("Overtaken_{platform}_{architecture}{version}.zip");
    let zip_path = Path::new("./export").join(&zip_name);

    if zip_path.exists() {
        println!("Removing existing zip file...");
        remove_file(&zip_path).unwrap();
    }

    println!("Packing to {zip_name}...");
    let zip_file = File::create(&zip_path).unwrap();
    let mut zip_writer = ZipWriter::new(&zip_file);

    let zip_options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o755);

    let walk = ignore::WalkBuilder::new(&export_dir).build();

    for entry in walk {
        let entry = entry.unwrap();
        let absolute = entry.path();
        let relative = absolute.strip_prefix(&export_dir).unwrap();

        if absolute.is_dir() {
            if relative.as_os_str().is_empty() {
                // Skipping root directory
                continue;
            }
            zip_writer
                .add_directory(relative.to_str().unwrap(), zip_options)
                .unwrap();
        } else {
            println!("Compressing {}...", relative.display());
            let mut input_file = File::open(absolute).unwrap();
            zip_writer
                .start_file(relative.to_str().unwrap(), zip_options)
                .unwrap();

            std::io::copy(&mut input_file, &mut zip_writer).unwrap();
        }
    }
    zip_writer.finish().unwrap();

    println!("Exported to {}!", zip_path.display());
}
