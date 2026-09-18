use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        if args[1] == "__internal-trampoline" {
            #[cfg(target_os = "linux")]
            {
                let code = boxr::runtime::linux::run_trampoline(&args[2..])?;
                std::process::exit(code);
            }
            #[cfg(not(target_os = "linux"))]
            {
                std::process::exit(0);
            }
        }
        if args[1] == "__internal-trampoline-exec" {
            #[cfg(target_os = "linux")]
            {
                let code = boxr::runtime::linux::run_trampoline_exec(&args[2..])?;
                std::process::exit(code);
            }
            #[cfg(not(target_os = "linux"))]
            {
                std::process::exit(0);
            }
        }
        if args[1] == "unshare" {
            #[cfg(target_os = "linux")]
            {
                let code = boxr::runtime::linux::run_unshare_cli(&args[2..])?;
                std::process::exit(code);
            }
        }
    }

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    rt.block_on(async {
        let cli = boxr::cli::Cli::parse();
        let code = boxr::run_cli(cli).await?;
        std::process::exit(code);
    })
}
