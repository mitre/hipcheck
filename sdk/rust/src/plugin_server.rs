// SPDX-License-Identifier: Apache-2.0

use crate::{
	error::{Error, Result},
	plugin_engine::HcSessionSocket,
	proto::{
		plugin_service_server::{PluginService, PluginServiceServer},
		DefaultPolicyExprRequest, DefaultPolicyExprResponse, Empty, ExplainDefaultQueryRequest,
		ExplainDefaultQueryResponse, QueryRequest, QueryResponse, QuerySchemasRequest,
		QuerySchemasResponse, SetConfigRequest, SetConfigResponse,
	},
	Plugin, QuerySchema,
};
use std::{result::Result as StdResult, sync::Arc};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream as RecvStream;
use tonic::{transport::Server, Code, Request as Req, Response as Resp, Status, Streaming};

/// Runs the Hipcheck plugin protocol based on the user's implementation of the `Plugin` trait.
///
/// This struct implements the underlying gRPC protocol that is not exposed to the plugin author.
pub struct PluginServer<P> {
	plugin: Arc<P>,
}

impl<P: Plugin> PluginServer<P> {
	/// Create a new plugin server for the provided plugin.
	pub fn register(plugin: P) -> PluginServer<P> {
		PluginServer {
			plugin: Arc::new(plugin),
		}
	}

	/// Run the plugin server on the provided port.
	pub async fn listen(self, port: u16) -> Result<()> {
		let service = PluginServiceServer::new(self);
		let host = format!("127.0.0.1:{}", port).parse().unwrap();

		Server::builder()
			.add_service(service)
			.serve(host)
			.await
			.map_err(Error::FailedToStartServer)?;

		Ok(())
	}
}

/// The result of running a query, where the error is of the type `tonic::Status`.
pub type QueryResult<T> = StdResult<T, Status>;

#[tonic::async_trait]
impl<P: Plugin> PluginService for PluginServer<P> {
	type QuerySchemasStream = RecvStream<QueryResult<QuerySchemasResponse>>;
	type QueryStream = RecvStream<QueryResult<QueryResponse>>;

	async fn set_config(&self, req: Req<SetConfigRequest>) -> QueryResult<Resp<SetConfigResponse>> {
		let config = serde_json::from_str(&req.into_inner().configuration)
			.map_err(|e| Status::from_error(Box::new(e)))?;
		self.plugin.set_config(config)?;
		Ok(Resp::new(SetConfigResponse {
			empty: Some(Empty {}),
		}))
	}

	async fn default_policy_expr(
		&self,
		_req: Req<DefaultPolicyExprRequest>,
	) -> QueryResult<Resp<DefaultPolicyExprResponse>> {
		// The request is empty, so we do nothing.
		match self.plugin.default_policy_expr() {
			Ok(policy_expression) => Ok(Resp::new(DefaultPolicyExprResponse { policy_expression })),
			Err(e) => Err(Status::new(
				tonic::Code::NotFound,
				format!(
					"Error determining default policy expr for {}/{}: {}",
					P::PUBLISHER,
					P::NAME,
					e
				),
			)),
		}
	}

	async fn explain_default_query(
		&self,
		_req: Req<ExplainDefaultQueryRequest>,
	) -> QueryResult<Resp<ExplainDefaultQueryResponse>> {
		match self.plugin.default_policy_expr() {
			Ok(explanation) => Ok(Resp::new(ExplainDefaultQueryResponse { explanation })),
			Err(e) => Err(Status::new(
				tonic::Code::NotFound,
				format!(
					"Error explaining default query expr for {}/{}: {}",
					P::PUBLISHER,
					P::NAME,
					e
				),
			)),
		}
	}

	async fn query_schemas(
		&self,
		_req: Req<QuerySchemasRequest>,
	) -> QueryResult<Resp<Self::QuerySchemasStream>> {
		// Ignore the input, it's empty.
		let query_schemas = self.plugin.schemas().collect::<Vec<QuerySchema>>();
		// TODO: does this need to be configurable?
		let (tx, rx) = mpsc::channel(10);
		tokio::spawn(async move {
			for x in query_schemas {
				let input_schema = serde_json::to_string(&x.input_schema);
				let output_schema = serde_json::to_string(&x.output_schema);

				let schema_resp = match (input_schema, output_schema) {
					(Ok(input_schema), Ok(output_schema)) => Ok(QuerySchemasResponse {
						query_name: x.query_name.to_string(),
						key_schema: input_schema,
						output_schema,
					}),
					(Ok(_), Err(e)) => Err(Status::new(
						Code::FailedPrecondition,
						format!("Error converting output schema to String: {}", e),
					)),
					(Err(_), Ok(e)) => Err(Status::new(
						Code::FailedPrecondition,
						format!("Error converting input schema to String: {}", e),
					)),
					(Err(e1), Err(e2)) => Err(Status::new(
						Code::FailedPrecondition,
						format!(
							"Error converting input and output schema to String: {} {}",
							e1, e2
						),
					)),
				};

				if tx.send(schema_resp).await.is_err() {
					// TODO: handle this?
					panic!();
				}
			}
		});
		Ok(Resp::new(RecvStream::new(rx)))
	}

	async fn query(
		&self,
		req: Req<Streaming<QueryRequest>>,
	) -> QueryResult<Resp<Self::QueryStream>> {
		let rx = req.into_inner();
		// TODO: - make channel size configurable
		let (tx, out_rx) = mpsc::channel::<QueryResult<QueryResponse>>(10);

		let cloned_plugin = self.plugin.clone();

		tokio::spawn(async move {
			let mut channel = HcSessionSocket::new(tx, rx);
			if let Err(e) = channel.run(cloned_plugin).await {
				panic!("Error: {e}");
			}
		});
		Ok(Resp::new(RecvStream::new(out_rx)))
	}
}
