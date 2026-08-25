use crate::cli_support::{
    install_crypto_provider, platform_to_proto, resolve_password, with_api_key, Cli, Commands,
    CredentialAction, ProviderAction,
};
use clap::Parser;
use std::path::{Path, PathBuf};

#[test]
fn install_crypto_provider_sets_process_default() {
    install_crypto_provider().expect("crypto provider installs");
    assert!(rustls::crypto::CryptoProvider::get_default().is_some());
}

#[test]
fn cli_tls_options_are_absent_by_default() {
    let cli =
        Cli::try_parse_from(["imauth", "status", "--session-id", "session-1"]).expect("valid CLI");
    assert_eq!(cli.tls_ca, None);
    assert_eq!(cli.tls_domain, None);
}

#[test]
fn cli_accepts_tls_ca_and_domain_options() {
    let cli = Cli::try_parse_from([
        "imauth",
        "--server",
        "https://127.0.0.1:7443",
        "--tls-ca",
        "test-ca.pem",
        "--tls-domain",
        "imauth.internal",
        "status",
        "--session-id",
        "session-1",
    ])
    .expect("valid CLI");
    assert_eq!(cli.tls_ca, Some(PathBuf::from("test-ca.pem")));
    assert_eq!(cli.tls_domain.as_deref(), Some("imauth.internal"));
}

#[test]
fn cli_exposes_cancel_validate_and_connections_commands() {
    let cancel = Cli::try_parse_from(["imauth", "cancel", "--session-id", "session-1"])
        .expect("valid cancel command");
    assert!(matches!(cancel.command, Commands::Cancel { .. }));

    let validate = Cli::try_parse_from(["imauth", "validate", "--platform", "instagram"])
        .expect("valid validate command");
    assert!(matches!(validate.command, Commands::Validate { .. }));

    let connections =
        Cli::try_parse_from(["imauth", "connections"]).expect("valid connections command");
    assert!(matches!(connections.command, Commands::Connections));
}

#[test]
fn provider_record_accepts_arbitrary_url_and_local_runtime_options() {
    let cli = Cli::try_parse_from([
        "imauth",
        "provider",
        "record",
        "--url",
        "https://nid.naver.com/nidlogin.login",
        "--domain",
        "naver.com",
        "--cdp-url",
        "http://127.0.0.1:9222",
        "--output-root",
        "tmp/provider-records",
        "--headless",
        "--auto-finish",
        "--deep",
    ])
    .expect("valid provider record command");

    assert!(matches!(
        cli.command,
        Commands::Provider {
            action: ProviderAction::Record {
                url,
                domain: Some(domain),
                cdp_url: Some(cdp_url),
                output_root,
                headless: true,
                auto_finish: true,
                deep: true,
            }
        } if url == "https://nid.naver.com/nidlogin.login"
            && domain == "naver.com"
            && cdp_url == "http://127.0.0.1:9222"
            && output_root == Path::new("tmp/provider-records")
    ));
}

#[test]
fn provider_record_uses_standard_detail_by_default() {
    let cli = Cli::try_parse_from([
        "imauth",
        "provider",
        "record",
        "--url",
        "https://example.com/login",
    ])
    .expect("valid standard provider record command");

    assert!(matches!(
        cli.command,
        Commands::Provider {
            action: ProviderAction::Record { deep: false, .. }
        }
    ));
}

#[test]
fn credential_save_accepts_password_from_environment_shape() {
    let cli = Cli::try_parse_from([
        "imauth",
        "credentials",
        "save",
        "--platform",
        "instagram",
        "--username",
        "user",
        "--password",
        "secret",
    ])
    .expect("valid credential command");
    assert!(matches!(
        cli.command,
        Commands::Credentials {
            action: CredentialAction::Save {
                password: Some(password),
                password_stdin: false,
                ..
            }
        } if password == "secret"
    ));
    assert_eq!(
        resolve_password(Some("secret".into()), false).expect("password"),
        "secret"
    );
}

#[test]
fn platform_to_proto_accepts_known_platforms_case_insensitive() {
    assert_eq!(platform_to_proto("instagram").expect("instagram"), 1);
    assert_eq!(platform_to_proto("Instagram").expect("Instagram"), 1);
    assert_eq!(platform_to_proto("THREADS").expect("THREADS"), 2);
    assert_eq!(platform_to_proto("naver").expect("naver"), 3);
    assert_eq!(platform_to_proto("NAVER").expect("NAVER"), 3);
    assert_eq!(platform_to_proto("novelpia"), Ok(4));
    assert_eq!(platform_to_proto("MUNPIA"), Ok(5));
}

#[test]
fn platform_to_proto_rejects_unknown_platform_with_helpful_message() {
    let error = platform_to_proto("facebook").expect_err("unsupported platform");
    assert!(error.contains("facebook"));
    assert!(error.contains("naver"));
    assert!(error.contains("novelpia"));
    assert!(error.contains("munpia"));
}

#[test]
fn with_api_key_does_not_set_authorization_when_key_is_none() {
    let request = with_api_key(tonic::Request::new(()), &None).expect("valid request");
    assert!(request.metadata().get("authorization").is_none());
}

#[test]
fn with_api_key_sets_bearer_authorization_when_key_present() {
    let request = with_api_key(tonic::Request::new(()), &Some("secret-key".to_string()))
        .expect("valid request");
    let value = request
        .metadata()
        .get("authorization")
        .expect("authorization header")
        .to_str()
        .expect("ASCII metadata");
    assert_eq!(value, "Bearer secret-key");
}
