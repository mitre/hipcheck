mod transport;
mod proto {
	include!(concat!(env!("OUT_DIR"), "/hipcheck.v1.rs"));
}

use crate::{
	proto::{
		plugin_service_server::{PluginService, PluginServiceServer},
		ConfigurationStatus, GetDefaultPolicyExpressionRequest, GetDefaultPolicyExpressionResponse,
		GetQuerySchemasRequest, GetQuerySchemasResponse, InitiateQueryProtocolRequest,
		InitiateQueryProtocolResponse, SetConfigurationRequest, SetConfigurationResponse,
	},
	transport::*,
};
use anyhow::{anyhow, Result};
use clap::Parser;
use serde_json::{json, Value};
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{transport::Server, Request, Response, Status, Streaming};

static GET_RAND_KEY_SCHEMA: &str = include_str!("../schema/query_schema_get_rand.json");
static GET_RAND_OUTPUT_SCHEMA: &str = include_str!("../schema/query_schema_get_rand.json");

fn reduce(input: u64) -> u64 {
	input % 7
}

pub async fn handle_rand_data(mut session: QuerySession, key: u64) -> Result<()> {
	let id = session.id();
	let sha_input = reduce(key);
	eprintln!("RAND-{id}: key: {key}, reduced: {sha_input}");

	let sha_req = Query {
		direction: QueryDirection::Request,
		publisher: "MITRE".to_owned(),
		plugin: "sha256".to_owned(),
		query: "sha256".to_owned(),
		key: json!(vec![sha_input]),
		output: json!(null),
	};

	session.send(sha_req).await?;
	let Some(res) = session.recv().await? else {
		return Err(anyhow!("channel closed prematurely by remote"));
	};

	if res.direction == QueryDirection::Request {
		return Err(anyhow!("expected response from remote"));
	}

	let mut sha_vec: Vec<u8> = serde_json::from_value(res.output)?;
	eprintln!("RAND-{id}: hash: {sha_vec:02x?}");
	for (sha_val, key_val) in Iterator::zip(sha_vec.iter_mut(), key.to_le_bytes()) {
		*sha_val += key_val;
	}

	eprintln!("RAND-{id}: output: {sha_vec:02x?}");
	let output = serde_json::to_value(sha_vec)?;

	let resp = Query {
		direction: QueryDirection::Response,
		publisher: "".to_owned(),
		plugin: "".to_owned(),
		query: "".to_owned(),
		key: json!(null),
		output,
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

	if name != "rand_data" {
		return Err(anyhow!("unrecognized query '{}'", name));
	}

	let Value::Number(num_size) = &key else {
		return Err(anyhow!("get_rand argument must be a number"));
	};

	let Some(size) = num_size.as_u64() else {
		return Err(anyhow!("get_rand argument must be an unsigned integer"));
	};

	handle_rand_data(session, size).await?;

	Ok(())
}

struct RandDataRunner {
	channel: HcSessionSocket,
}

impl RandDataRunner {
	pub fn new(channel: HcSessionSocket) -> Self {
		RandDataRunner { channel }
	}

	pub async fn run(mut self) -> Result<()> {
		loop {
			eprintln!("RAND: Looping");

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
struct RandDataPlugin {
	pub schema: GetQuerySchemasResponse,
}

impl RandDataPlugin {
	pub fn new() -> Self {
		let schema = GetQuerySchemasResponse {
			query_name: "rand_data".to_owned(),
			key_schema: GET_RAND_KEY_SCHEMA.to_owned(),
			output_schema: GET_RAND_OUTPUT_SCHEMA.to_owned(),
		};

		RandDataPlugin { schema }
	}
}

#[tonic::async_trait]
impl PluginService for RandDataPlugin {
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

	async fn initiate_query_protocol(
		&self,
		request: Request<Streaming<InitiateQueryProtocolRequest>>,
	) -> Result<Response<Self::InitiateQueryProtocolStream>, Status> {
		let rx = request.into_inner();
		let (tx, out_rx) = mpsc::channel::<Result<InitiateQueryProtocolResponse, Status>>(4);

		tokio::spawn(async move {
			let channel = HcSessionSocket::new(tx, rx);
			if let Err(e) = RandDataRunner::new(channel).run().await {
				eprintln!("rand_data plugin ended in error: {e}");
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
	let addr = format!("127.0.0.1:{}", args.port);
	let plugin = RandDataPlugin::new();
	let svc = PluginServiceServer::new(plugin);

	Server::builder()
		.add_service(svc)
		.serve(addr.parse().unwrap())
		.await?;

	Ok(())
}
