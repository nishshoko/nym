use std::path::PathBuf;

use client_core::config::GatewayEndpoint;
use std::sync::Arc;
use tap::TapFallible;
use tokio::sync::RwLock;

use client_core::config::Config as BaseConfig;
use config_common::NymConfig;
use nym_socks5::client::config::Config as Socks5Config;

use crate::{
    error::{BackendError, Result},
    state::State,
};

static SOCKS5_CONFIG_ID: &str = "nym-connect";

pub fn socks5_config_id_appended_with(gateway_id: &str) -> Result<String> {
    use std::fmt::Write as _;
    let mut id = SOCKS5_CONFIG_ID.to_string();
    write!(id, "-{}", gateway_id)?;
    Ok(id)
}

#[tauri::command]
pub async fn get_config_id(state: tauri::State<'_, Arc<RwLock<State>>>) -> Result<String> {
    state.read().await.get_config_id()
}

#[tauri::command]
pub async fn get_config_file_location(
    state: tauri::State<'_, Arc<RwLock<State>>>,
) -> Result<String> {
    let id = get_config_id(state).await?;
    Config::config_file_location(&id).map(|d| d.to_string_lossy().to_string())
}

#[derive(Debug)]
pub struct Config {
    socks5: Socks5Config,
}

impl Config {
    pub fn new<S: Into<String>>(id: S, provider_mix_address: S) -> Self {
        Config {
            socks5: Socks5Config::new(id, provider_mix_address),
        }
    }

    pub fn get_socks5(&self) -> &Socks5Config {
        &self.socks5
    }

    #[allow(unused)]
    pub fn get_socks5_mut(&mut self) -> &mut Socks5Config {
        &mut self.socks5
    }

    pub fn get_base(&self) -> &BaseConfig<Socks5Config> {
        self.socks5.get_base()
    }

    pub fn get_base_mut(&mut self) -> &mut BaseConfig<Socks5Config> {
        self.socks5.get_base_mut()
    }

    pub async fn init(service_provider: &str, chosen_gateway_id: &str) -> Result<()> {
        log::info!("Initialising...");

        let service_provider = service_provider.to_owned();
        let chosen_gateway_id = chosen_gateway_id.to_owned();

        // The client initialization was originally not written for this use case, so there are
        // lots of ways it can panic. Until we have proper error handling in the init code for the
        // clients we'll catch any panics here by spawning a new runtime in a separate thread.
        std::thread::spawn(move || {
            tokio::runtime::Runtime::new()
                .expect("Failed to create tokio runtime")
                .block_on(
                    async move { init_socks5_config(service_provider, chosen_gateway_id).await },
                )
        })
        .join()
        .map_err(|_| BackendError::InitializationPanic)??;

        log::info!("Configuration saved 🚀");
        Ok(())
    }

    pub fn config_file_location(id: &str) -> Result<PathBuf> {
        Socks5Config::try_default_config_file_path(Some(id))
            .ok_or(BackendError::CouldNotGetConfigFilename)
    }
}

pub async fn init_socks5_config(provider_address: String, chosen_gateway_id: String) -> Result<()> {
    log::trace!("Initialising client...");

    // Append the gateway id to the name id that we store the config under
    let id = socks5_config_id_appended_with(&chosen_gateway_id)?;

    log::debug!(
        "Attempting to use config file location: {}",
        Config::config_file_location(&id)?.to_string_lossy(),
    );
    let already_init = Config::config_file_location(&id)?.exists();
    if already_init {
        log::info!(
            "SOCKS5 client \"{}\" was already initialised before! \
            Config information will be overwritten (but keys will be kept)!",
            id
        );
    }

    // Future proofing. This flag exists for the other clients
    let user_wants_force_register = false;

    let register_gateway = !already_init || user_wants_force_register;

    log::trace!("Creating config for id: {}", id);
    let mut config = Config::new(id.as_str(), &provider_address);

    if let Ok(raw_validators) = std::env::var(config_common::defaults::var_names::API_VALIDATOR) {
        config
            .get_base_mut()
            .set_custom_validator_apis(config_common::parse_validators(&raw_validators));
    }

    let gateway = setup_gateway(
        &id,
        register_gateway,
        Some(&chosen_gateway_id),
        config.get_socks5(),
    )
    .await?;
    config.get_base_mut().with_gateway_endpoint(gateway);

    let config_save_location = config.get_socks5().get_config_file_save_location();
    config.get_socks5().save_to_file(None).tap_err(|_| {
        log::error!("Failed to save the config file");
    })?;

    log::info!("Saved configuration file to {:?}", config_save_location);
    log::info!("Gateway id: {}", config.get_base().get_gateway_id());
    log::info!("Gateway owner: {}", config.get_base().get_gateway_owner());
    log::info!(
        "Gateway listener: {}",
        config.get_base().get_gateway_listener()
    );

    log::info!(
        "Service provider address: {}",
        config.get_socks5().get_provider_mix_address()
    );
    log::info!(
        "Service provider port: {}",
        config.get_socks5().get_listening_port()
    );
    log::info!("Client configuration completed.");

    client_core::init::show_address(config.get_base())?;
    Ok(())
}

// TODO: deduplicate with same functions in other client
async fn setup_gateway(
    id: &str,
    register: bool,
    user_chosen_gateway_id: Option<&str>,
    config: &Socks5Config,
) -> Result<GatewayEndpoint> {
    if register {
        // Get the gateway details by querying the validator-api. Either pick one at random or use
        // the chosen one if it's among the available ones.
        println!("Configuring gateway");
        let gateway = client_core::init::query_gateway_details(
            config.get_base().get_validator_api_endpoints(),
            user_chosen_gateway_id,
        )
        .await?;
        log::debug!("Querying gateway gives: {}", gateway);

        // Registering with gateway by setting up and writing shared keys to disk
        log::trace!("Registering gateway");
        client_core::init::register_with_gateway_and_store_keys(gateway.clone(), config.get_base())
            .await?;
        println!("Saved all generated keys");

        Ok(gateway.into())
    } else if user_chosen_gateway_id.is_some() {
        // Just set the config, don't register or create any keys
        // This assumes that the user knows what they are doing, and that the existing keys are
        // valid for the gateway being used
        println!("Using gateway provided by user, keeping existing keys");
        let gateway = client_core::init::query_gateway_details(
            config.get_base().get_validator_api_endpoints(),
            user_chosen_gateway_id,
        )
        .await?;
        log::debug!("Querying gateway gives: {}", gateway);
        Ok(gateway.into())
    } else {
        println!("Not registering gateway, will reuse existing config and keys");
        let existing_config = Socks5Config::load_from_file(Some(id)).map_err(|err| {
            log::error!(
                "Unable to configure gateway: {err}. \n
                Seems like the client was already initialized but it was not possible to read \
                the existing configuration file. \n
                CAUTION: Consider backing up your gateway keys and try force gateway registration, or \
                removing the existing configuration and starting over."
            );
            BackendError::CouldNotLoadExistingGatewayConfiguration(err)
        })?;
        Ok(existing_config.get_base().get_gateway_endpoint().clone())
    }
}
