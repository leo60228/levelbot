use async_std::prelude::*;
use async_trait::async_trait;
use cloudflare::framework::{
    async_api::ApiClient,
    auth,
    endpoint::{Endpoint, Method},
    response::{ApiErrors, ApiFailure, ApiResponse, ApiResult, ApiSuccess},
    Environment, HttpApiClientConfig,
};
use serde::{Serialize, Deserialize};
use std::convert::TryInto;
use surf::http::StatusCode;

struct SurfApiClient {
    environment: Environment,
    credentials: auth::Credentials,
    config: HttpApiClientConfig,
}

impl SurfApiClient {
    pub fn new(
        credentials: auth::Credentials,
        config: HttpApiClientConfig,
        environment: Environment,
    ) -> Self {
        Self {
            environment,
            credentials,
            config,
        }
    }
}

#[async_trait]
impl ApiClient for SurfApiClient {
    async fn request<ResultType, QueryType, BodyType>(
        &self,
        endpoint: &(dyn Endpoint<ResultType, QueryType, BodyType> + Send + Sync),
    ) -> ApiResponse<ResultType>
    where
        ResultType: ApiResult,
        QueryType: Serialize,
        BodyType: Serialize,
    {
        let url = endpoint.url(&self.environment);
        let mut request = match endpoint.method() {
            Method::Get => surf::get(url),
            Method::Post => surf::post(url),
            Method::Put => surf::put(url),
            Method::Delete => surf::delete(url),
            Method::Patch => surf::patch(url),
        };
        request = request.set_query(&endpoint.query()).unwrap();
        if let Some(body) = endpoint.body() {
            request = request.body_json(&body).unwrap();
        }
        for (k, v) in self.credentials.headers() {
            request = request.set_header(k, v);
        }
        let mut resp = request
            .timeout(self.config.http_timeout)
            .await
            .unwrap()
            .unwrap();
        if resp.status() == StatusCode::OK {
            Ok(resp.body_json::<ApiSuccess<ResultType>>().await.unwrap())
        } else {
            let errors = resp.body_json::<ApiErrors>().await.unwrap_or_default();
            Err(ApiFailure::Error(
                resp.status().as_u16().try_into().unwrap(),
                errors,
            ))
        }
    }
}

pub struct PurgeCacheByUrl<'a> {
    pub identifier: &'a str,
    pub urls: Vec<&'a str>,
}

#[derive(Serialize)]
pub struct PurgeCacheByUrlsParams<'a> {
    pub files: Vec<&'a str>,
}

#[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PurgeCacheByUrlsResponse {
    pub id: String,
}

impl ApiResult for PurgeCacheByUrlsResponse {}

impl<'a> Endpoint<PurgeCacheByUrlsResponse, (), PurgeCacheByUrlsParams<'a>> for PurgeCacheByUrl<'a> {
    fn method(&self) -> Method {
        Method::Post
    }

    fn path(&self) -> String {
        format!("zones/{}/purge_cache", self.identifier)
    }

    fn body(&self) -> Option<PurgeCacheByUrlsParams<'a>> {
        Some(PurgeCacheByUrlsParams {
            files: self.urls.clone(),
        })
    }
}

pub async fn purge_cache() {
    let email = dotenv::var("CLOUDFLARE_EMAIL").unwrap();
    let key = dotenv::var("CLOUDFLARE_KEY").unwrap();
    let zone = dotenv::var("CLOUDFLARE_ZONE").unwrap();
    let url = dotenv::var("CLOUDFLARE_LEVELS_URL").unwrap();
    let creds = auth::Credentials::UserAuthKey { email, key };
    let api_client = SurfApiClient::new(creds, Default::default(), Environment::Production);
    let purge = PurgeCacheByUrl {
        identifier: &zone,
        urls: vec![&url],
    };
    let _ = api_client.request(&purge).await.unwrap();
}
