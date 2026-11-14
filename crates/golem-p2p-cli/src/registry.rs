//! `golem-p2p registry`: run and manage the registry of a closed network.

use std::{net::SocketAddr, path::PathBuf, time::Duration};

use anyhow::{Context, bail};
use clap::{Args, Subcommand};
use golem_p2p::{NodeId, RegistryClient, registry::RegistryState};

use crate::{out::Out, parse, write_secret};

#[derive(Subcommand)]
pub enum RegistryCmd {
    /// Create a new registry state file: a fresh signing key and admin token.
    /// Prints the network name and public key (give both to every node) and
    /// writes the admin token to `--admin-token-out`.
    Init {
        /// State file to create (JSON, mode 0600). Keep it secret: it holds
        /// the signing key.
        #[arg(long)]
        state: PathBuf,
        /// Network name, part of every certificate.
        #[arg(long, default_value = "golem-testnet")]
        network: String,
        /// Lifetime of issued certificates. A node removed from the registry
        /// loses access at most this long (plus 5 min clock tolerance) later.
        #[arg(long, default_value = "1h", value_parser = parse::duration)]
        cert_lifetime: Duration,
        /// File to write the admin token to (mode 0600, must not exist).
        #[arg(long)]
        admin_token_out: PathBuf,
    },
    /// Serve the registry over HTTP (put a TLS-terminating proxy in front).
    Serve {
        /// State file created by `registry init`.
        #[arg(long)]
        state: PathBuf,
        /// Address to listen on.
        #[arg(long, default_value = "127.0.0.1:8090")]
        listen: SocketAddr,
    },
    /// Print a registry's network name, public key and certificate lifetime.
    Info {
        #[command(flatten)]
        at: RegistryAt,
    },
    /// Manage accounts (admin token).
    Account {
        #[command(flatten)]
        at: RegistryAt,
        /// File with the admin token.
        #[arg(long, env = "GOLEM_P2P_ADMIN_TOKEN_FILE", global = true)]
        admin_token_file: Option<PathBuf>,
        #[command(subcommand)]
        cmd: AccountCmd,
    },
    /// Register and remove your nodes (account token).
    Node {
        #[command(flatten)]
        at: RegistryAt,
        /// File with your account token (from `registry account add`).
        #[arg(long, env = "GOLEM_P2P_TOKEN_FILE", global = true)]
        token_file: Option<PathBuf>,
        #[command(subcommand)]
        cmd: NodeCmd,
    },
}

#[derive(Args)]
pub struct RegistryAt {
    /// Registry URL.
    #[arg(long, env = "GOLEM_P2P_REGISTRY", global = true)]
    registry: Option<url::Url>,
}

#[derive(Subcommand)]
pub enum AccountCmd {
    /// Create an account and write its token to `--token-out`.
    Add {
        name: String,
        /// File to write the account's token to (mode 0600, must not exist).
        #[arg(long)]
        token_out: PathBuf,
    },
    /// Remove an account and all its nodes.
    Remove { name: String },
    /// List accounts with their number of nodes.
    List,
}

#[derive(Subcommand)]
pub enum NodeCmd {
    /// Register a node ID (Ethereum address) to your account.
    Add { node_id: NodeId },
    /// Remove one of your node IDs.
    Remove { node_id: NodeId },
    /// List your node IDs.
    List,
}

fn read_token(path: Option<&PathBuf>, flag: &str) -> anyhow::Result<String> {
    let Some(path) = path else {
        bail!("{flag} is required");
    };
    Ok(std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?
        .trim()
        .to_string())
}

fn client(at: &RegistryAt) -> anyhow::Result<RegistryClient> {
    let Some(url) = &at.registry else {
        bail!("--registry is required (or GOLEM_P2P_REGISTRY)");
    };
    Ok(RegistryClient::new(url.clone())?)
}

/// Runs a call and fails with the registry's message unless the status is
/// in `ok`.
async fn call(
    c: &RegistryClient,
    method: &str,
    path: &str,
    token: Option<&str>,
) -> anyhow::Result<String> {
    let (status, body) = c.call(method, path, token).await?;
    if !(200..300).contains(&status) {
        bail!(
            "registry answered {}",
            golem_p2p::membership::error_text(status, &body)
        );
    }
    Ok(body)
}

pub async fn run(cmd: RegistryCmd, out: &Out) -> anyhow::Result<()> {
    match cmd {
        RegistryCmd::Init {
            state,
            network,
            cert_lifetime,
            admin_token_out,
        } => {
            if state.exists() {
                bail!("{} already exists", state.display());
            }
            if admin_token_out.exists() {
                bail!("{} already exists", admin_token_out.display());
            }
            let (st, admin) = RegistryState::init(&network, cert_lifetime)?;
            st.save(&state)?;
            write_secret(&admin_token_out, &format!("{admin}\n"))?;
            let key = hex::encode(st.policy().registry_keys()[0].as_bytes());
            if out.json() {
                out.event("registry_init")
                    .field("network", network)
                    .field("public_key", key)
                    .emit();
            } else {
                println!("network={network}\npublic_key={key}");
                println!(
                    "admin token written to {}; nodes need --network {network} --registry-key {key}",
                    admin_token_out.display()
                );
            }
        }
        RegistryCmd::Serve { state, listen } => {
            // Fail early on a bad state file.
            let st = RegistryState::load(&state)?;
            let listener = tokio::net::TcpListener::bind(listen).await?;
            let url = format!("http://{}/", listener.local_addr()?);
            let key = hex::encode(st.policy().registry_keys()[0].as_bytes());
            if out.json() {
                out.event("registry")
                    .field("url", url)
                    .field("network", st.network.clone())
                    .field("public_key", key)
                    .emit();
            } else {
                println!(
                    "registry of network {} listening on {url}, public key {key}",
                    st.network
                );
            }
            tokio::select! {
                r = golem_p2p::registry::serve(listener, state) => r?,
                _ = crate::shutdown_signal() => {}
            }
        }
        RegistryCmd::Info { at } => {
            print!("{}", call(&client(&at)?, "GET", "v1/info", None).await?);
        }
        RegistryCmd::Account {
            at,
            admin_token_file,
            cmd,
        } => {
            let c = client(&at)?;
            let admin = read_token(admin_token_file.as_ref(), "--admin-token-file")?;
            match cmd {
                AccountCmd::Add { name, token_out } => {
                    if token_out.exists() {
                        bail!("{} already exists", token_out.display());
                    }
                    let token =
                        call(&c, "POST", &format!("v1/accounts/{name}"), Some(&admin)).await?;
                    write_secret(&token_out, &format!("{}\n", token.trim()))?;
                    println!(
                        "account {name} created; token written to {}",
                        token_out.display()
                    );
                }
                AccountCmd::Remove { name } => {
                    call(&c, "DELETE", &format!("v1/accounts/{name}"), Some(&admin)).await?;
                    println!("account {name} and its nodes removed");
                }
                AccountCmd::List => {
                    print!("{}", call(&c, "GET", "v1/accounts", Some(&admin)).await?)
                }
            }
        }
        RegistryCmd::Node {
            at,
            token_file,
            cmd,
        } => {
            let c = client(&at)?;
            let token = read_token(token_file.as_ref(), "--token-file")?;
            match cmd {
                NodeCmd::Add { node_id } => {
                    call(&c, "PUT", &format!("v1/nodes/{node_id}"), Some(&token)).await?;
                    println!("{node_id} registered");
                }
                NodeCmd::Remove { node_id } => {
                    call(&c, "DELETE", &format!("v1/nodes/{node_id}"), Some(&token)).await?;
                    println!("{node_id} removed; it loses access when its certificate expires");
                }
                NodeCmd::List => print!("{}", call(&c, "GET", "v1/nodes", Some(&token)).await?),
            }
        }
    }
    Ok(())
}
