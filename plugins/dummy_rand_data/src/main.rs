#![allow(unused_variables)]

mod hipcheck;
mod hipcheck_transport;

use crate::hipcheck_transport::*;
use anyhow::{anyhow, Result};
use clap::Parser;
use hipcheck::plugin_server::{Plugin, PluginServer};
use hipcheck::{
	Configuration, ConfigurationResult, ConfigurationStatus, Empty, PolicyExpression,
	Query as PluginQuery, Schema,
};
use serde_json::{json, Value};
use std::pin::Pin;
use tokio::sync::mpsc;
use tokio_stream::{wrappers::ReceiverStream, Stream};
use tonic::{transport::Server, Request, Response, Status, Streaming};

static GET_RAND_KEY_SCHEMA: &str = include_str!("query_schema_get_rand.json");
static GET_RAND_OUTPUT_SCHEMA: &str = include_str!("query_schema_get_rand.json");

fn reduce(input: u64) -> u64 {
	input % 7
}

pub async fn handle_rand_data(mut session: QuerySession, key: u64) -> Result<()> {
	let id = session.id();
	let sha_input = reduce(key);
	eprintln!("RAND-{id}: key: {key}, reduced: {sha_input}");

	let sha_req = Query {
		request: true,
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

	if res.request {
		return Err(anyhow!("expected response from remote"));
	}

	let mut sha_vec: Vec<u8> = serde_json::from_value(res.output)?;
	eprintln!("RAND-{id}: hash: {sha_vec:02x?}");
	let key_vec = key.to_le_bytes().to_vec();

	for (i, val) in key_vec.into_iter().enumerate() {
		*sha_vec.get_mut(i).unwrap() += val;
	}

	eprintln!("RAND-{id}: output: {sha_vec:02x?}");
	let output = serde_json::to_value(sha_vec)?;

	let resp = Query {
		request: false,
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

	if !query.request {
		return Err(anyhow!("Expected request from remote"));
	}

	let name = query.query;
	let key = query.key;

	if name == "rand_data" {
		let Value::Number(num_size) = &key else {
			return Err(anyhow!("get_rand argument must be a number"));
		};

		let Some(size) = num_size.as_u64() else {
			return Err(anyhow!("get_rand argument must be an unsigned integer"));
		};

		handle_rand_data(session, size).await?;

		Ok(())
	} else {
		Err(anyhow!("unrecognized query '{}'", name))
	}
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
	pub schema: Schema,
}

impl RandDataPlugin {
	pub fn new() -> Self {
		let schema = Schema {
			query_name: "rand_data".to_owned(),
			key_schema: GET_RAND_KEY_SCHEMA.to_owned(),
			output_schema: GET_RAND_OUTPUT_SCHEMA.to_owned(),
		};
		RandDataPlugin { schema }
	}
}

#[tonic::async_trait]
impl Plugin for RandDataPlugin {
	type GetQuerySchemasStream =
		Pin<Box<dyn Stream<Item = Result<Schema, Status>> + Send + 'static>>;
	type InitiateQueryProtocolStream = ReceiverStream<Result<PluginQuery, Status>>;

	async fn get_query_schemas(
		&self,
		_request: Request<Empty>,
	) -> Result<Response<Self::GetQuerySchemasStream>, Status> {
		Ok(Response::new(Box::pin(tokio_stream::iter(vec![Ok(self
			.schema
			.clone())]))))
	}

	async fn set_configuration(
		&self,
		request: Request<Configuration>,
	) -> Result<Response<ConfigurationResult>, Status> {
		Ok(Response::new(ConfigurationResult {
			status: ConfigurationStatus::ErrorNone as i32,
			message: "".to_owned(),
		}))
	}

	async fn get_default_policy_expression(
		&self,
		request: Request<Empty>,
	) -> Result<Response<PolicyExpression>, Status> {
		Ok(Response::new(PolicyExpression {
			policy_expression: "".to_owned(),
		}))
	}

	async fn initiate_query_protocol(
		&self,
		request: Request<Streaming<PluginQuery>>,
	) -> Result<Response<Self::InitiateQueryProtocolStream>, Status> {
		let rx = request.into_inner();
		let (tx, out_rx) = mpsc::channel::<Result<PluginQuery, Status>>(4);

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
	let svc = PluginServer::new(plugin);

	Server::builder()
		.add_service(svc)
		.serve(addr.parse().unwrap())
		.await?;

	Ok(())
}
