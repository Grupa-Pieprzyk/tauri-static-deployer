use clap::{
    Parser,
    Subcommand,
};
use enum_iterator::IntoEnumIterator;
use eyre::{
    bail,
    Result,
    WrapErr,
};
use itertools::Itertools;
use release_notes_file::{
    ReleasePlatformV1,
    ReleasePlatformV2,
};
use s3_helpers::s3_handler::handle_s3;
use s3_helpers::{
    s3_handler,
    S3Config,
};
use serde::{
    Deserialize,
    Serialize,
};
use std::path::Path;
use std::{
    path::PathBuf,
    str::FromStr,
};
use tauri_conf_json::TauriConfJson;
use tracing::instrument;

#[allow(unused_imports)]
use tracing::{
    debug,
    error,
    info,
    trace,
    warn,
};

use crate::{
    namespacing::{
        derive_binary_file_s3_key,
        derive_release_file_s3_key,
    },
    release_notes_file::RemoteRelease,
};

macro_rules! env_required {
    ($env:literal) => {
        std::env::var($env).wrap_err(format!("{} missing", $env))?
    };
}

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, IntoEnumIterator,
)]
pub enum RustTarget {
    #[serde(rename = "i686-pc-windows-msvc")]
    Win32,
    #[serde(rename = "x86_64-pc-windows-msvc")]
    Win64,
    #[serde(rename = "x86_64-unknown-linux-gnu")]
    Linux64,
}

impl RustTarget {
    pub fn to_release_platform(&self) -> Result<Vec<release_notes_file::ReleasePlatform>> {
        match self {
            RustTarget::Win32 => Ok(vec![
                release_notes_file::ReleasePlatform::V1(ReleasePlatformV1::Win32),
                release_notes_file::ReleasePlatform::V2(ReleasePlatformV2::Win32),
            ]),
            RustTarget::Win64 => Ok(vec![
                release_notes_file::ReleasePlatform::V1(ReleasePlatformV1::Win64),
                release_notes_file::ReleasePlatform::V2(ReleasePlatformV2::Win64),
            ]),
            RustTarget::Linux64 => Ok(vec![
                release_notes_file::ReleasePlatform::V1(ReleasePlatformV1::Linux),
                release_notes_file::ReleasePlatform::V2(ReleasePlatformV2::Linux),
            ]),
        }
    }
}

#[derive(
    Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, IntoEnumIterator,
)]
#[serde(rename_all = "snake_case")]
pub enum RustChannel {
    Nightly,
    Stable,
}
macro_rules! matched_variant {
    ($Self:ty, $v:expr) => {{
        Self::into_enum_iter()
            .find(|v| serde_variant::to_variant_name(v).expect("bad variant?") == $v)
            .ok_or(eyre::eyre!(
                "{} hasn't matched any variant of {}",
                $v,
                std::any::type_name::<$Self>()
            ))
    }};
}
impl FromStr for RustChannel {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        matched_variant!(Self, s)
    }
}

impl FromStr for RustTarget {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        matched_variant!(Self, s)
    }
}

mod release_notes_file {
    use std::collections::HashMap;

    use time::OffsetDateTime;

    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum ReleasePlatformV1 {
        #[serde(rename = "win64")]
        Win64,
        #[serde(rename = "win32")]
        Win32,
        #[serde(rename = "linux")]
        Linux,
    }
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub enum ReleasePlatformV2 {
        #[serde(rename = "windows-x86_64")]
        Win64,
        #[serde(rename = "windows-i686")]
        Win32,
        #[serde(rename = "linux-x86_64")]
        Linux,
    }

    #[derive(
        Debug,
        Clone,
        Serialize,
        Deserialize,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        derive_more::From,
    )]
    #[serde(untagged)]
    pub enum ReleasePlatform {
        V1(ReleasePlatformV1),
        V2(ReleasePlatformV2),
    }

    impl ReleasePlatform {
        pub fn to_installer_str(&self) -> String {
            // match self {
            //     ReleasePlatform::Win64 => "x64",
            //     ReleasePlatform::Win32 => "x86",
            //     ReleasePlatform::Linux => unimplemented!("this platform is not supported"),
            // }
            // .to_owned()

            match self {
                ReleasePlatform::V1(r) => match r {
                    ReleasePlatformV1::Win64 => "x64",
                    ReleasePlatformV1::Win32 => "x86",
                    ReleasePlatformV1::Linux => {
                        unimplemented!("linux platform is not supported at the moment")
                    }
                },
                ReleasePlatform::V2(r) => match r {
                    ReleasePlatformV2::Win64 => "x64",
                    ReleasePlatformV2::Win32 => "x86",
                    ReleasePlatformV2::Linux => {
                        unimplemented!("linux platform is not supported at the moment")
                    }
                },
            }
            .to_owned()
        }
    }
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct RemoteRelease {
        pub url: String,
        pub signature: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ReleaseNotes {
        // pub name: String,
        /// 'name' is deprecated, prefer using version. version must be semver compatible
        pub version: String,
        pub notes: String,
        #[serde(with = "serde_pub_date")]
        pub pub_date: OffsetDateTime,
        pub platforms: HashMap<ReleasePlatform, RemoteRelease>,
    }

    mod serde_pub_date {
        use serde::{
            Deserialize,
            Deserializer,
            Serializer,
        };
        use time::format_description::well_known::Rfc3339;
        use time::OffsetDateTime;
        const FORMAT: Rfc3339 = Rfc3339;

        pub fn deserialize<'de, D>(d: D) -> Result<OffsetDateTime, D::Error>
        where
            D: Deserializer<'de>,
        {
            let date = String::deserialize(d)?;
            OffsetDateTime::parse(&date, &FORMAT).map_err(|e| {
                serde::de::Error::custom(format!("invalid value for `OffsetDateTime`: {}", e))
            })
        }
        pub fn serialize<S>(val: &OffsetDateTime, ser: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let val: String = val.format(&FORMAT).map_err(|e| {
                serde::ser::Error::custom(format!(
                    "date {val:?} is not serializable to RFC format {FORMAT:?}: {e:?}"
                ))
            })?;
            ser.serialize_str(&val)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use pretty_assertions::assert_eq;
        #[test]
        fn test_generated_release_works() -> eyre::Result<()> {
            let example = ReleaseNotes {
                version: "1.2.3".to_string(),
                notes: "test".to_string(),
                pub_date: OffsetDateTime::now_utc(),
                platforms: Default::default(),
            };

            let serialized = serde_json::to_string_pretty(&example).wrap_err("serializing")?;
            eprintln!("{serialized}");
            let deserialized: ReleaseNotes =
                serde_json::from_str(&serialized).wrap_err("deserializing")?;
            assert_eq!(example.pub_date, deserialized.pub_date);
            Ok(())
        }
        #[test]
        fn check_current_release_file_works() -> eyre::Result<()> {
            const CURRENT: &str = include_str!("../test_data/release-notes.json");
            let parsed: ReleaseNotes =
                serde_json::from_str(CURRENT).wrap_err("could not parse the original")?;
            assert_eq!(
                serde_json::to_string_pretty(&parsed)
                    .wrap_err("could not serialize")?
                    .trim(),
                CURRENT.trim()
            );
            Ok(())
        }
    }
}

pub mod tauri_conf_json {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct Package {
        pub product_name: String,
        pub version: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Updater {
        pub endpoints: Vec<String>,
        #[serde(flatten)]
        pub rest: serde_json::Value,
    }
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Tauri {
        pub updater: Updater,
        pub bundle: Bundle,
        #[serde(flatten)]
        pub rest: serde_json::Value,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Bundle {
        pub identifier: String,
        #[serde(flatten)]
        pub rest: serde_json::Value,
    }
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct TauriConfJson {
        pub package: Package,
        pub tauri: Tauri,
        #[serde(flatten)]
        pub rest: serde_json::Value,
    }

    impl TauriConfJson {
        pub fn with_update_endpoint(&mut self, endpoint: String) -> &mut Self {
            let old = self.tauri.updater.endpoints.clone();
            self.tauri.updater.endpoints = vec![endpoint];
            info!(
                "tauri.updater.endpoints :: {:?} -> {:?}",
                old, self.tauri.updater.endpoints
            );
            self
        }

        pub fn with_update_identifier(&mut self, identifier: String) -> &mut Self {
            let old = self.tauri.bundle.identifier.clone();

            self.tauri.bundle.identifier = identifier;
            info!(
                "tauri.bundle.identifier :: {:?} -> {:?}",
                old, self.tauri.bundle.identifier
            );
            self
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use eyre::{
            Context,
            Result,
        };
        const CONTENT: &str = include_str!("../test_data/tauri.conf.json");
        #[test]
        fn test_file_loads() -> Result<()> {
            let original: serde_json::Value =
                serde_json::from_str(CONTENT).wrap_err("failed to parse tauri.conf.json")?;
            let parsed: TauriConfJson =
                serde_json::from_str(CONTENT).wrap_err("failed to parse tauri.conf.json")?;
            let reparsed: serde_json::Value =
                serde_json::from_str(&serde_json::to_string_pretty(&parsed)?)?;
            println!("{reparsed:#?}");
            assert_eq!(original, reparsed);
            Ok(())
        }
    }
}

// pub mod s3_handler {
//     use eyre::bail;
//     pub mod handle_s3 {

//         use super::*;

//         pub fn s3_url_prefix(config: &S3Config) -> String {
//             format!(
//                 "https://{}.{}.digitaloceanspaces.com",
//                 config.bucket.name, config.bucket.region
//             )
//         }
//         pub async fn upload_to_s3<T: AsRef<Path>>(
//             file: T,
//             config: &S3Config,
//             s3_path: &str,
//         ) -> eyre::Result<String> {
//             info!("sending to s3 :: {} [{}]", file.as_ref().display(), s3_path);
//             let mut path = tokio::fs::File::open(&file)
//                 .await
//                 .wrap_err("failed to open file for sending to S3")?;
//             let code = config
//                 .bucket
//                 .put_object_stream(&mut path, s3_path)
//                 .await
//                 .wrap_err(format!(
//                     "failed to send file to S3: {}",
//                     file.as_ref().display()
//                 ))?;
//             if code != 200 {
//                 bail!(
//                     "S3 returned non-200 code for [{}] -> [{}]",
//                     file.as_ref().display(),
//                     s3_path
//                 )
//             }
//             let url = format!("{}/{}", s3_url_prefix(config), s3_path);
//             info!("SUCCESS :: new asset available under [{url}]");
//             Ok(url)
//         }
//     }

//     use super::*;
//     #[derive(Debug, Clone)]
//     pub struct S3Config {
//         pub bucket: s3::Bucket,
//     }

//     impl S3Config {
//         pub fn try_from_env() -> eyre::Result<Self> {
//             let access_key = env_required!("S3_ACCESS_KEY");
//             let secret_key = env_required!("S3_SECRET_KEY");
//             let bucket = env_required!("S3_BUCKET");
//             let region = env_required!("S3_REGION");
//             let credentials = s3::creds::Credentials::new(
//                 Some(access_key.as_str()),
//                 Some(secret_key.as_str()),
//                 None,
//                 None,
//                 None,
//             )
//             .wrap_err("bad s3 credentials")?;

//             let region = s3::Region::Custom {
//                 endpoint: format!("{region}.digitaloceanspaces.com"),
//                 region,
//             };
//             let mut bucket =
//                 s3::Bucket::new(bucket.as_str(), region, credentials).wrap_err("bad bucket")?;
//             bucket.add_header("x-amz-acl", "public-read");
//             Ok(Self { bucket })
//         }

//         pub async fn upload_to_subdirectory<T: AsRef<Path>>(
//             &self,
//             file: T,
//             s3_path: &str,
//         ) -> eyre::Result<String> {
//             handle_s3::upload_to_s3(file, self, s3_path).await
//         }
//     }
// }

pub mod metadata {

    use super::*;

    fn fix_newlines(val: &str) -> String {
        val.trim_end_matches("\r\n")
            .trim_end_matches("\n\r")
            .trim_end_matches('\r')
            .trim_end_matches('\n')
            .trim_end_matches('\r')
            .trim_end_matches('\n')
            .to_string()
            .replace('\r', "")
    }

    #[cfg(target_os = "windows")]
    pub fn decode_command_output(bytes: &[u8]) -> Result<String> {
        use encoding::Encoding;
        match encoding::all::WINDOWS_1252.decode(bytes, encoding::DecoderTrap::Ignore) {
            Ok(v) => Ok(fix_newlines(&v)),
            Err(e) => Err(eyre::eyre!("failed to decode windows output :: {:?}", e)),
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn decode_command_output(bytes: &[u8]) -> Result<String> {
        String::from_utf8(bytes.to_vec())
            .wrap_err("failed to decode linux output")
            .map(|s| fix_newlines(&s))
    }

    pub fn current_target() -> Result<RustTarget> {
        let out = std::process::Command::new("rustup")
            .arg("default")
            .output()
            .wrap_err("running command to get current target")?;

        let text = decode_command_output(&out.stdout).wrap_err("bad encoding")?;
        let default_target = text
            .lines()
            .find(|line| line.contains("default"))
            .ok_or_else(|| eyre::eyre!("no default target found"))?;
        let (channel, target) = default_target
            .split_once('-')
            .ok_or_else(|| eyre::eyre!("bad format for target"))?;
        let (target, _) = target
            .split_once(' ')
            .ok_or_else(|| eyre::eyre!("bad format for target"))?;
        let _channel: RustChannel = channel.parse()?;
        let target = target.parse()?;
        Ok(target)
    }

    #[instrument(ret, level = "debug")]
    pub fn current_branch() -> Result<String> {
        let out = std::process::Command::new("git")
            .arg("branch")
            .arg("--show-current")
            .output()
            .wrap_err("getting current branch")?;

        decode_command_output(&out.stdout)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn check_current_target() -> Result<()> {
            current_target()?;
            Ok(())
        }
        #[test]
        fn check_current_branch() -> Result<()> {
            println!("detected current branch: [{}]", current_branch()?);
            Ok(())
        }
    }
}
pub mod namespacing {
    use super::*;
    #[instrument(ret)]
    pub fn derive_release_base_key(branch_name: &str, target: &RustTarget) -> String {
        format!(
            "{}/{}",
            branch_name,
            serde_variant::to_variant_name(&target).expect("this will always serialize")
        )
    }

    #[instrument(ret)]
    pub fn derive_release_file_s3_key(branch_name: &str, target: &RustTarget) -> String {
        format!(
            "{}/release-notes.json",
            derive_release_base_key(branch_name, target)
        )
    }

    #[instrument(ret)]
    pub fn derive_release_file_s3_url(
        branch_name: &str,
        target: &RustTarget,
        s3_config: &S3Config,
    ) -> String {
        use s3_handler::handle_s3::{
            s3_path_with_subdirectory,
            s3_url,
        };
        s3_url(
            s3_config,
            &s3_path_with_subdirectory(s3_config, &derive_release_file_s3_key(branch_name, target)),
        )
    }

    #[instrument(ret, skip(binary_file_path), fields(binary_file_parh=%binary_file_path.as_ref().display()))]
    pub fn derive_binary_file_s3_key<T: AsRef<Path>>(
        tauri_conf_json: &TauriConfJson,
        target: &RustTarget,
        branch_name: &str,
        binary_file_path: T,
        git_commit_hash: &str,
    ) -> Result<String> {
        let filename = binary_file_path
            .as_ref()
            .to_path_buf()
            .file_name()
            .ok_or_else(|| eyre::eyre!("this is a directory"))?
            .to_string_lossy()
            .to_string();
        Ok(format!(
            "{}/{}/{git_commit_hash}/{}",
            derive_release_base_key(branch_name, target),
            &tauri_conf_json.package.version,
            filename
        ))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use eyre::Result;
        use s3_helpers::BucketConfig;

        #[test]
        fn test_release_file_s3_path() -> Result<()> {
            const TAURI_CONF_JSON: &str = include_str!("../test_data/tauri.conf.json");
            let _tauri_conf_json: TauriConfJson =
                serde_json::from_str(TAURI_CONF_JSON).wrap_err("bad format for tauri.conf.json")?;
            assert_eq!(
                derive_release_file_s3_key("release", &RustTarget::Linux64),
                "release/x86_64-unknown-linux-gnu/release-notes.json"
            );
            Ok(())
        }
        #[test]
        fn test_release_file_s3_url() -> Result<()> {
            assert_eq!(
                derive_release_file_s3_url(
                    "release", 
                    &RustTarget::Win64, 
                    &S3Config { 
                        bucket_subdirectory: "test-bucket-subdirectory".to_string(), 
                        bucket_config: BucketConfig { 
                            name: "test-bucket-name".to_string(), 
                            region_name: "us-east-1".to_string(),
                        }, 
                        account_id: "it-doesnt-matter".to_string(), 
                        bucket: None, 
                        actual_domain: "https://test-bucket-name.blazingsoft.pl".to_string(),
                    }),
                "https://test-bucket-name.blazingsoft.pl/test-bucket-subdirectory/release/x86_64-pc-windows-msvc/release-notes.json"
            );
            Ok(())
        }
    }
}
const DEFAULT_TAURI_CONF_JSON_PATH: &str = "./src-tauri/tauri.conf.json";

/// should return "./src-tauri/target/release/bundle/"
fn release_assets_path(target: &RustTarget) -> Result<PathBuf> {
    let base = PathBuf::from_str("./src-tauri")
        .wrap_err("bad base path")?
        .join("target");
    let for_target = base.join(serde_variant::to_variant_name(target).wrap_err("bad variant?")?);
    let target_base = if for_target.exists() {
        for_target
    } else {
        base.join("release")
    };
    Ok(target_base.join("bundle"))
}

#[derive(Subcommand, Debug)]
enum Command {
    /// must be run before tauri action, tauri.conf.json needs to be patched in order for updater to reference the correct S3 release manifest file.
    Patch,
    /// this builds and publishes the release according to s3 config
    Upload {
        #[clap(short, long, value_name = "DIR")]
        release_dir: Option<PathBuf>,
        /// this stage also cleans up release artifacts after uploading them - by default rust-cache action saves them all which makes the cache grow out of control
        #[clap(short, long)]
        cleanup: bool,
    },
}

/// CI script for easier tauri app deployment
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
struct Args {
    #[clap(long, default_value_t = String::from(DEFAULT_TAURI_CONF_JSON_PATH), value_name = "FILE")]
    /// path to tauri.conf.json
    tauri_conf_json_path: String,
    #[clap(long)]
    /// override rust target
    target: Option<RustTarget>,
    #[clap(subcommand)]
    command: Command,
}

fn git_hash() -> Result<String> {
    let output = std::process::Command::new("git")
        .args(&["rev-parse", "HEAD"])
        .output()
        .context("faled to read git hash")?;
    let git_hash = String::from_utf8(output.stdout)
        .map(|v| v.trim().to_string())
        .context("faled to read git hash")
        .and_then(|v| {
            if v.is_empty() {
                Err(eyre::eyre!("empty git commit"))
            } else {
                Ok(v)
            }
        })?
        .chars()
        .take(8)
        .collect();
    Ok(git_hash)
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv::dotenv().ok();
    color_eyre::install().ok();
    tracing_subscriber::fmt::init();
    let args = Args::parse();
    let path = args.tauri_conf_json_path;
    let git_hash = git_hash().unwrap_or_else(|e| {
        warn!("no commit hash: {e:?}");
        warn!("using uuid instead");
        uuid::Uuid::new_v4().to_string()
    });
    // tauri.conf.json
    let tauri_conf_json_path = PathBuf::from_str(&path).wrap_err("parsing tauri.conf.json path")?;
    let mut tauri_conf_json: TauriConfJson = std::fs::read_to_string(&tauri_conf_json_path)
        .wrap_err("reading tauri.conf.json")
        .and_then(|content| serde_json::from_str(&content).wrap_err("parsing tauri.conf.json"))?;
    // metadata
    let branch = metadata::current_branch().wrap_err("getting branch name")?;
    let target = match args.target {
        Some(t) => t,
        None => {
            let target =
                metadata::current_target().wrap_err("getting rust from environment target")?;
            warn!("target not set, using {target:?}");
            target
        }
    };
    let release_platforms = target
        .to_release_platform()
        .wrap_err("getting release platform from target")?;
    // s3 config
    let s3_config = S3Config::try_from_env()
        .map_err(|e| eyre::eyre!("{e:?}"))
        .wrap_err("getting s3 config from env")?;

    debug!(?s3_config);
    match args.command {
        Command::Patch => {
            info!("patching {}", tauri_conf_json_path.display());
            let new_identifier = format!(
                "{}.{}",
                tauri_conf_json.tauri.bundle.identifier,
                branch.replace('/', "_").replace(' ', "_").replace(':', "_")
            );
            tauri_conf_json
                .with_update_endpoint(namespacing::derive_release_file_s3_url(
                    &branch,
                    &target,
                    &s3_config,
                ))
                .with_update_identifier(new_identifier);
        }
        Command::Upload {
            release_dir,
            cleanup,
        } => {
            let release_dir = match release_dir {
                Some(r) => r,
                None => release_assets_path(&target).wrap_err("failed to derive a release path")?,
            };

            let files = walkdir::WalkDir::new(release_dir)
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .wrap_err("reading release dir entries")?
                .into_iter()
                .filter(|e| e.path().is_file())
                .map(|entry| entry.path().canonicalize().wrap_err("absolute path"))
                .collect::<Result<Vec<_>, _>>()
                .wrap_err("getting absolute paths")?;
            let with_keys = files
                .iter()
                .map(|binary_file_path| {
                    derive_binary_file_s3_key(
                        &tauri_conf_json,
                        &target,
                        &branch,
                        binary_file_path.clone(),
                        &git_hash,
                    )
                    .map(|key| (binary_file_path, key))
                })
                .collect::<Result<Vec<_>, _>>()
                .wrap_err("extracting s3 keys")?;
            info!("uploading:\n{:#?}", with_keys);
            let tasks = with_keys
                .iter()
                .map(|(path, key)| {
                    handle_s3::upload_to_s3(
                        path,
                        &s3_config,
                        handle_s3::s3_path_with_subdirectory(&s3_config, key),
                    )
                })
                .collect_vec();
            let urls = futures::future::try_join_all(tasks)
                .await
                .map_err(|e| eyre::eyre!("{e:?}"))
                .wrap_err("uploading all binary files")?;
            info!("all files uploaded");
            if cleanup {
                warn!("cleaning up to prevent cache from growing out of control");
                files
                    .into_iter()
                    .map(|path| {
                        std::fs::remove_file(&path)
                            .wrap_err(format!("cleaning up [{}]", path.display()))
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .wrap_err("cleaning up cache")?;
            }

            let binary_url = urls
                .iter()
                .find(|url| url.ends_with(".zip"))
                .ok_or_else(|| eyre::eyre!("getting zip file"))?; // TODO: this is only for windows
            let signature = {
                match urls
                    .iter()
                    .find(|url| url.ends_with(".zip.sig")) // TODO: this is only for windows
                    .ok_or_else(|| eyre::eyre!("getting sig file"))
                {
                    Ok(signature_url) => reqwest::get(signature_url)
                        .await
                        .wrap_err("downloading signature content")?
                        .text()
                        .await
                        .wrap_err("reading signature content")?,
                    Err(e) => {
                        error!("{e} :: failed to read signature file. in newer version of tauri this will result in an error. setting signature as \"\" (empty string)");
                        String::new()
                    }
                }
            };

            let release = release_notes_file::ReleaseNotes {
                notes: format!(
                    "new {} release: {}",
                    branch, tauri_conf_json.package.version
                ),
                version: tauri_conf_json.package.version.clone(),
                // notes: "released new version".to_string(), // TODO: customise this
                pub_date: time::OffsetDateTime::now_utc(),
                platforms: release_platforms
                    .into_iter()
                    .map(|release_platform| {
                        (
                            release_platform,
                            RemoteRelease {
                                url: binary_url.clone(),
                                signature: signature.clone(),
                            },
                        )
                    })
                    .collect(), // platforms: []
                                // .into_iter()
                                // .collect(),
            };
            info!(
                " :: uploading release ::\n{}\n\n",
                serde_json::to_string_pretty(&release).unwrap_or_default()
            );
            let release_local_path = {
                let path = PathBuf::from_str("./")
                    .wrap_err("this should work")?
                    .join("TEMP_RELEASE_FILE.json");
                std::fs::write(
                    path.clone(),
                    serde_json::to_string_pretty(&release).wrap_err("serializing release file")?,
                )
                .wrap_err("dumping release file to a file")?;
                path
            };
            let release_key = derive_release_file_s3_key(&branch, &target);
            info!("binaries upload successfully, generating release_file");
            let release_file_url = handle_s3::upload_to_s3(
                release_local_path,
                &s3_config,
                handle_s3::s3_path_with_subdirectory(&s3_config, &release_key),
            )
            .await
            .map_err(|e| eyre::eyre!("{e:?}"))
            .wrap_err("uploading release file to s3")?;

            info!(" :: validating ::");
            if !tauri_conf_json
                .tauri
                .updater
                .endpoints
                .iter()
                .any(|url| url == &release_file_url)
            {
                error!("CRITICAL ERROR! UPDATE WILL NOT BE TRIGGERED!");
                bail!("configuration error - release file url is '{release_file_url}', but no such endpoint was found in tauri.conf.json file. entries found: {:?}", &tauri_conf_json.tauri.updater.endpoints)
            }

            info!(" ::: uploaded to [{release_key}], update is LIVE :::");
        }
    }

    serde_json::to_string_pretty(&tauri_conf_json)
        .wrap_err("serializing tauri.conf.json content")
        .and_then(|conf| {
            info!("writing to {:?}:\n\n{}\n\n", tauri_conf_json_path, conf);
            std::fs::write(tauri_conf_json_path, &conf).wrap_err("saving tauri.conf.json")
        })?;
    info!("DONE");
    Ok(())
}
