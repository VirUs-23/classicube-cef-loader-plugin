use crate::{error::*, print_async};
use futures::stream::TryStreamExt;
use std::{
    fs, io,
    io::{Read, Write},
    marker::Unpin,
    path::{Component, Path, PathBuf},
};
use tokio::prelude::*;

pub const CEF_VERSION: &str = "cef_binary_81.2.16+gdacda4f+chromium-81.0.4044.92_windows64_minimal";
pub const CEF_BINARY_PATH: &str = r"cef\cef_binary";
pub const CEF_BINARY_VERSION_PATH: &str = r"cef\cef_binary.txt";

fn get_current_version() -> Option<String> {
    fs::File::open(CEF_BINARY_VERSION_PATH)
        .map(|mut f| {
            let mut s = String::new();
            f.read_to_string(&mut s).unwrap();
            s
        })
        .ok()
}

pub async fn check() -> Result<()> {
    let current_version = get_current_version().unwrap_or_default();

    if current_version != CEF_VERSION {
        print_async(format!("Updating cef-binary to {}", CEF_VERSION)).await;

        fs::create_dir_all(CEF_BINARY_PATH).unwrap();
        download(CEF_VERSION).await?;

        {
            // mark as updated
            let mut f = fs::File::create(CEF_BINARY_VERSION_PATH)?;
            write!(f, "{}", CEF_VERSION).unwrap();
        }

        print_async("cef-binary finished downloading, restart your game to finish the update!")
            .await;
    }

    Ok(())
}

struct FuturesBlockOnReader<R>
where
    R: AsyncRead,
{
    async_reader: R,
}

impl<R> Read for FuturesBlockOnReader<R>
where
    R: AsyncRead + Unpin,
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        futures::executor::block_on(self.async_reader.read(buf))
    }
}

async fn download(version: &str) -> Result<()> {
    use futures::compat::{Compat, Compat01As03};
    use tokio_util::compat::{FuturesAsyncReadCompatExt, Tokio02AsyncReadCompatExt};

    let url = format!(
        "http://opensource.spotify.com/cefbuilds/{}.tar.bz2",
        version
    )
    .replace("+", "%2B");

    let stream = reqwest::get(&url).await?.bytes_stream();

    let stream =
        tokio::io::stream_reader(stream.map_err(|e| io::Error::new(io::ErrorKind::Other, e)));

    let stream = tokio::io::BufReader::new(stream);
    let stream = Tokio02AsyncReadCompatExt::compat(stream);

    let stream = Compat::new(stream);
    let decoder = bzip2::read::BzDecoder::new(stream);
    let decoder = Compat01As03::new(decoder);

    let decoder = FuturesAsyncReadCompatExt::compat(decoder);
    let decoder = tokio::io::BufReader::new(decoder);

    let bad_reader = FuturesBlockOnReader {
        async_reader: decoder,
    };

    tokio::task::spawn_blocking(move || {
        let mut archive = tar::Archive::new(bad_reader);

        let mut cef_binary_name: Option<String> = None;

        for file in archive.entries()? {
            let mut file = file?;
            let path = file.path()?.to_owned();
            let mut components = path.components();

            // remove cef_binary_* part
            let first_component = if let Some(Component::Normal(component)) = components.next() {
                component.to_str().unwrap().to_string()
            } else {
                unreachable!();
            };

            // check we always have the same first directory
            if let Some(cef_binary_name) = &cef_binary_name {
                assert!(cef_binary_name == &first_component);
            } else {
                cef_binary_name = Some(first_component);
            }

            let trimmed_path: PathBuf = components
                .inspect(|part| {
                    if let Component::Normal(_) = part {
                    } else {
                        // don't allow anything but Normal
                        unreachable!();
                    }
                })
                .collect();

            let mut trimmed_path_components = trimmed_path.components();

            if let Some(Component::Normal(first_part)) = trimmed_path_components.next() {
                if let Some(ext) = trimmed_path.extension() {
                    if (first_part == "Release" && (ext == "dll" || ext == "bin"))
                        || (first_part == "Resources" && (ext == "pak" || ext == "dat"))
                    {
                        let even_more_trimmed: PathBuf = trimmed_path_components.collect();
                        // icu .dat and .bin files must be next to cef.dll
                        let out_path = Path::new(CEF_BINARY_PATH).join(&even_more_trimmed);
                        println!("{:?} {:?}", path, out_path);
                        fs::create_dir_all(out_path.parent().unwrap())?;
                        file.unpack(&out_path)?;
                    }
                }
            }
        }

        Ok::<(), Error>(())
    })
    .await??;

    Ok(())
}