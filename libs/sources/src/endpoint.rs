use std::fmt;

use lodestone_core::{LodestoneUseCase, SourceError, provider_descriptor};
use url::Url;

#[derive(Clone)]
pub struct CatalogEndpoint {
    usecase: LodestoneUseCase,
    provider: &'static str,
    field: &'static str,
    fallback: Url,
}

#[derive(Clone)]
pub enum Endpoint {
    Fixed(Url),
    Catalog(CatalogEndpoint),
}

impl From<Url> for Endpoint {
    fn from(url: Url) -> Self {
        Self::Fixed(url)
    }
}

impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fixed(url) => write!(f, "Fixed({url})"),
            Self::Catalog(catalog) => write!(
                f,
                "Catalog({}.{}, fallback {})",
                catalog.provider, catalog.field, catalog.fallback
            ),
        }
    }
}

fn unavailable(message: impl fmt::Display) -> SourceError {
    SourceError::Upstream(format!("provider catalog: {message}"))
}

impl Endpoint {
    pub fn catalog(
        usecase: LodestoneUseCase,
        provider: &'static str,
        field: &'static str,
        fallback: Url,
    ) -> Self {
        Self::Catalog(CatalogEndpoint {
            usecase,
            provider,
            field,
            fallback,
        })
    }

    pub async fn resolve(&self) -> Result<Url, SourceError> {
        match self {
            Self::Fixed(url) => Ok(url.clone()),
            Self::Catalog(catalog) => catalog.resolve().await,
        }
    }
}

impl CatalogEndpoint {
    async fn resolve(&self) -> Result<Url, SourceError> {
        let configuration = self
            .usecase
            .get_provider_configuration(self.provider)
            .await
            .map_err(unavailable)?;
        let Some(configuration) = configuration else {
            return Ok(self.fallback.clone());
        };
        let descriptor = provider_descriptor(self.provider)
            .ok_or_else(|| unavailable(format!("{} is not in the catalog", self.provider)))?;
        let default = descriptor
            .field(self.field)
            .and_then(|field| field.default)
            .unwrap_or_default();
        let raw = descriptor.endpoint(&configuration.settings, self.field, default);
        Url::parse(raw).map_err(|error| {
            unavailable(format!(
                "{} setting {} is not a url: {error}",
                self.provider, self.field
            ))
        })
    }
}
