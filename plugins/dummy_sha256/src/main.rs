// SPDX-License-Identifier: Apache-2.0

mod transport;
mod proto {
	include!(concat!(env!("OUT_DIR"), "/hipcheck.v1.rs"));
}

use crate::{
	proto::{
		plugin_service_server::{PluginService, PluginServiceServer},
		ConfigurationStatus, GetDefaultPolicyExpressionRequest, GetQuerySchemasRequest,
		SetConfigurationRequest, SetConfigurationResponse,
	},
	transport::*,
};
use anyhow::{anyhow, Result};
use clap::Parser;
use proto::{
	ExplainDefaultQueryRequest, ExplainDefaultQueryResponse, GetDefaultPolicyExpressionResponse,
	GetQuerySchemasResponse, InitiateQueryProtocolRequest, InitiateQueryProtocolResponse,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{transport::Server, Request, Response, Status, Streaming};

static SHA256_KEY_SCHEMA: &str = include_str!("../schema/query_schema_sha256.json");
static SHA256_OUTPUT_SCHEMA: &str = include_str!("../schema/query_schema_sha256.json");

fn sha256(content: &[u8]) -> Vec<u8> {
	let mut hasher = Sha256::new();
	hasher.update(content);
	hasher.finalize().to_vec()
}

async fn handle_sha256(session: QuerySession, key: &[u8]) -> Result<()> {
	eprintln!("Key: {key:02x?}");
	let res = sha256(key);

	eprintln!("Hash: {res:02x?}");
	let output = serde_json::to_value(res)?;

	let resp = Query {
		direction: QueryDirection::Response,
		publisher: "".to_owned(),
		plugin: "".to_owned(),
		query: "".to_owned(),
		key: json!(null),
		output,
		concerns: vec![],
	};

	session.send(resp).await?;

	Ok(())
}

async fn handle_session(mut session: QuerySession) -> Result<()> {
	let Some(query) = session.recv().await? else {
		eprintln!("session closed by remote");
		return Ok(());
	};

	if query.direction == QueryDirection::Response {
		return Err(anyhow!("Expected request from remote"));
	}

	let name = query.query;
	let key = query.key;

	if name != "sha256" {
		return Err(anyhow!("unrecognized query '{}'", name));
	}

	let Value::Array(data) = &key else {
		return Err(anyhow!("get_sha256 argument must be an array"));
	};

	let data = data
		.iter()
		.map(|elem| elem.as_u64().map(|num| num as u8))
		.collect::<Option<Vec<_>>>()
		.ok_or_else(|| anyhow!("non-numeric data in get_sha256 array argument"))?;

	handle_sha256(session, &data[..]).await?;

	Ok(())
}

struct Sha256Runner {
	channel: HcSessionSocket,
}

impl Sha256Runner {
	pub fn new(channel: HcSessionSocket) -> Self {
		Sha256Runner { channel }
	}

	pub async fn run(mut self) -> Result<()> {
		loop {
			eprintln!("SHA256: Looping");

			let Some(session) = self.channel.listen().await? else {
				eprintln!("Channel closed by remote");
				break;
			};

			tokio::spawn(async move {
				if let Err(e) = handle_session(session).await {
					eprintln!("handle_session failed: {e}");
				};
			});
		}

		Ok(())
	}
}

#[derive(Debug)]
struct Sha256Plugin {
	schema: GetQuerySchemasResponse,
}

impl Sha256Plugin {
	fn new() -> Self {
		Sha256Plugin {
			schema: GetQuerySchemasResponse {
				query_name: "sha256".to_owned(),
				key_schema: SHA256_KEY_SCHEMA.to_owned(),
				output_schema: SHA256_OUTPUT_SCHEMA.to_owned(),
			},
		}
	}
}

#[tonic::async_trait]
impl PluginService for Sha256Plugin {
	type GetQuerySchemasStream =
		Pin<Box<dyn Stream<Item = Result<GetQuerySchemasResponse, Status>> + Send + 'static>>;

	type InitiateQueryProtocolStream =
		ReceiverStream<Result<InitiateQueryProtocolResponse, Status>>;

	async fn get_query_schemas(
		&self,
		_request: Request<GetQuerySchemasRequest>,
	) -> Result<Response<Self::GetQuerySchemasStream>, Status> {
		Ok(Response::new(Box::pin(tokio_stream::iter(vec![Ok(self
			.schema
			.clone())]))))
	}

	async fn set_configuration(
		&self,
		_request: Request<SetConfigurationRequest>,
	) -> Result<Response<SetConfigurationResponse>, Status> {
		Ok(Response::new(SetConfigurationResponse {
			status: ConfigurationStatus::None as i32,
			message: "".to_owned(),
		}))
	}

	async fn get_default_policy_expression(
		&self,
		_request: Request<GetDefaultPolicyExpressionRequest>,
	) -> Result<Response<GetDefaultPolicyExpressionResponse>, Status> {
		Ok(Response::new(GetDefaultPolicyExpressionResponse {
			policy_expression: "".to_owned(),
		}))
	}

	async fn explain_default_query(
		&self,
		_request: Request<ExplainDefaultQueryRequest>,
	) -> Result<Response<ExplainDefaultQueryResponse>, Status> {
		Ok(Response::new(ExplainDefaultQueryResponse {
			explanation: "perform SHA-256 hashing of the input".to_owned(),
		}))
	}

	async fn initiate_query_protocol(
		&self,
		request: Request<Streaming<InitiateQueryProtocolRequest>>,
	) -> Result<Response<Self::InitiateQueryProtocolStream>, Status> {
		let rx = request.into_inner();
		let (tx, out_rx) = mpsc::channel::<Result<InitiateQueryProtocolResponse, Status>>(4);

		tokio::spawn(async move {
			let channel = HcSessionSocket::new(tx, rx);

			if let Err(e) = Sha256Runner::new(channel).run().await {
				eprintln!("sha256 plugin ended in error: {e}");
			}
		});

		Ok(Response::new(ReceiverStream::new(out_rx)))
	}
}

#[derive(Parser, Debug)]
struct Args {
	#[arg(long)]
	port: u16,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let args = Args::try_parse().map_err(Box::new)?;

	let service = PluginServiceServer::new(Sha256Plugin::new());
	let host = format!("127.0.0.1:{}", args.port).parse().unwrap();

	Server::builder().add_service(service).serve(host).await?;

	Ok(())
}
