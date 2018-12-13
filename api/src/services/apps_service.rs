/*-
 * ========================LICENSE_START=================================
 * PREvant
 * %%
 * Copyright (C) 2018 aixigo AG
 * %%
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 * =========================LICENSE_END==================================
 */
use std::convert::From;

use handlebars::TemplateRenderError;
use models::service::{ContainerType, Service, ServiceConfig, ServiceError};
use multimap::MultiMap;
use services::config_service::{Config, ConfigError};

use services::docker::docker_infrastructure::DockerInfrastructure;
use services::infrastructure::Infrastructure;
use services::service_templating::apply_templating_for_application_companion;

pub struct AppsService {
    config: Config,
    infrastructure: Box<dyn Infrastructure>,
}

impl AppsService {
    pub fn new() -> Result<AppsService, AppsServiceError> {
        Ok(AppsService {
            config: Config::load()?,
            infrastructure: Box::new(DockerInfrastructure::new()),
        })
    }

    /// Analyzes running containers and returns a map of `app-name` with the
    /// corresponding list of `Service`s.
    pub fn get_apps(&self) -> Result<MultiMap<String, Service>, AppsServiceError> {
        Ok(self.infrastructure.get_services()?)
    }

    /// Creates or updates a app to review with the given service configurations.
    ///
    /// The list of given services will be extended with:
    /// - the replications from the running template application (e.g. master)
    /// - the application companions (see README)
    /// - the service companions (see README)
    pub fn create_or_update(
        &self,
        app_name: &String,
        service_configs: &Vec<ServiceConfig>,
    ) -> Result<Vec<Service>, AppsServiceError> {
        let mut configs: Vec<ServiceConfig> = service_configs.clone();

        if "master" != app_name {
            for config in self
                .infrastructure
                .get_configs_of_app(&String::from("master"))?
                .iter()
                .filter(|config| {
                    match service_configs
                        .iter()
                        .find(|c| c.get_service_name() == config.get_service_name())
                    {
                        None => true,
                        Some(_) => false,
                    }
                })
            {
                let mut replicated_config = config.clone();
                replicated_config.set_container_type(ContainerType::Replica);
                configs.push(replicated_config);
            }
        }

        for app_companion_config in self.config.get_application_companion_configs()? {
            let applied_template_config = apply_templating_for_application_companion(
                &app_companion_config,
                app_name,
                &configs,
            );
            configs.push(applied_template_config?);
        }

        let services = self.infrastructure.start_services(
            app_name,
            &configs,
            &self.config.get_container_config(),
        )?;

        Ok(services)
    }

    /// Deletes all services for the given `app_name`.
    pub fn delete_app(&self, app_name: &String) -> Result<Vec<Service>, AppsServiceError> {
        match self.infrastructure.get_services()?.get_vec(app_name) {
            None => Err(AppsServiceError::AppNotFound(app_name.clone())),
            Some(_) => Ok(self.infrastructure.stop_services(app_name)?),
        }
    }
}

/// Defines error cases for the `AppService`
#[derive(Debug)]
pub enum AppsServiceError {
    /// Will be used when the service configuration is invalid that has been request by the client
    InvalidServiceModel(ServiceError),
    /// Will be used when no app with a given name is found
    AppNotFound(String),
    /// Will be used when the service cannot interact correctly with the infrastructure.
    InfrastructureError(failure::Error),
    /// Will be used if the service configuration cannot be loaded.
    InvalidServerConfiguration(ConfigError),
    InvalidTemplateFormat(TemplateRenderError),
}

impl From<ConfigError> for AppsServiceError {
    fn from(err: ConfigError) -> Self {
        AppsServiceError::InvalidServerConfiguration(err)
    }
}

impl From<failure::Error> for AppsServiceError {
    fn from(error: failure::Error) -> Self {
        AppsServiceError::InfrastructureError(error)
    }
}

impl From<TemplateRenderError> for AppsServiceError {
    fn from(error: TemplateRenderError) -> Self {
        AppsServiceError::InvalidTemplateFormat(error)
    }
}
