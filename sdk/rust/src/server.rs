// SPDX-License-Identifier: Apache-2.0

use crate::{
	engine::HcSessionSocket,
	error::{Error, Result},
	Plugin, QuerySchema,
};
use hipcheck_common::{
	proto::{
		plugin_service_server::{PluginService, PluginServiceServer},
		ConfigurationStatus, ExplainDefaultQueryRequest as ExplainDefaultQueryReq,
		ExplainDefaultQueryResponse as ExplainDefaultQueryResp,
		GetDefaultPolicyExpressionRequest as GetDefaultPolicyExpressionReq,
		GetDefaultPolicyExpressionResponse as GetDefaultPolicyExpressionResp,
		GetQuerySchemasRequest as GetQuerySchemasReq,
		GetQuerySchemasResponse as GetQuerySchemasResp,
		InitiateQueryProtocolRequest as InitiateQueryProtocolReq,
		InitiateQueryProtocolResponse as InitiateQueryProtocolResp,
		SetConfigurationRequest as SetConfigurationReq,
		SetConfigurationResponse as SetConfigurationResp,
	},
	types::LogLevel,
};
use std::{
	net::{Ipv4Addr, SocketAddr},
	result::Result as StdResult,
	sync::Arc,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream as RecvStream;
use tonic::{transport::Server, Code, Request as Req, Response as Resp, Status, Streaming};
use tracing::error;

#[derive(Debug, Clone)]
pub enum Host {
	// 127.0.0.1
	Loopback,
	// 0.0.0.0
	Any,
	// Any other IP address.
	Other(Ipv4Addr),
}

impl Host {
	fn to_socket_addr(&self, port: u16) -> SocketAddr {
		match self {
			Host::Loopback => SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), port),
			Host::Any => SocketAddr::new(Ipv4Addr::new(0, 0, 0, 0).into(), port),
			Host::Other(ip) => SocketAddr::new((*ip).into(), port),
		}
	}
}

/// Runs the Hipcheck plugin protocol based on the user's implementation of the `Plugin` trait.
///
/// This struct implements the underlying gRPC protocol that is not exposed to the plugin author.
pub struct PluginServer<P> {
	plugin: Arc<P>,
	curr_host: Host,
}

impl<P: Plugin> PluginServer<P> {
	/// Create a new plugin server for the provided plugin.
	pub fn register(plugin: P, log_level_opt: impl Into<Option<LogLevel>>) -> PluginServer<P> {
		#[cfg(feature = "log_forwarding")]
		{
			let log_level = log_level_opt.into().unwrap_or(LogLevel::Error);
			crate::init_tracing_logger(log_level);
		}

		PluginServer {
			plugin: Arc::new(plugin),
			curr_host: Host::Any, // default
		}
	}

	/// Run the plugin server on the loopback address and provided port.
	pub async fn listen_local(self, port: u16) -> Result<()> {
		self.listen(Host::Loopback, port).await
	}

	/// Run the plugin server on the provided port.
	pub async fn listen(mut self, host: Host, port: u16) -> Result<()> {
		self.curr_host = host.clone();
		let service = PluginServiceServer::new(self);
		let host_addr = host.to_socket_addr(port);

		Server::builder()
			.add_service(service)
			.serve(host_addr)
			.await
			.map_err(Error::FailedToStartServer)?;

		Ok(())
	}
}

/// The result of running a query, where the error is of the type `tonic::Status`.
pub type QueryResult<T> = StdResult<T, Status>;

#[tonic::async_trait]
impl<P: Plugin> PluginService for PluginServer<P> {
	type GetQuerySchemasStream = RecvStream<QueryResult<GetQuerySchemasResp>>;
	type InitiateQueryProtocolStream = RecvStream<QueryResult<InitiateQueryProtocolResp>>;

	async fn set_configuration(
		&self,
		req: Req<SetConfigurationReq>,
	) -> QueryResult<Resp<SetConfigurationResp>> {
		let config = serde_json::from_str(&req.into_inner().configuration)
			.map_err(|e| Status::from_error(Box::new(e)))?;
		match self.plugin.set_config(config) {
			Ok(_) => Ok(Resp::new(SetConfigurationResp {
				status: ConfigurationStatus::None as i32,
				message: "".to_owned(),
			})),
			Err(e) => Ok(Resp::new(e.into())),
		}
	}

	async fn get_default_policy_expression(
		&self,
		_req: Req<GetDefaultPolicyExpressionReq>,
	) -> QueryResult<Resp<GetDefaultPolicyExpressionResp>> {
		// The request is empty, so we do nothing.
		match self.plugin.default_policy_expr() {
			Ok(policy_expression) => Ok(Resp::new(GetDefaultPolicyExpressionResp {
				policy_expression,
			})),
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
		_req: Req<ExplainDefaultQueryReq>,
	) -> QueryResult<Resp<ExplainDefaultQueryResp>> {
		match self.plugin.explain_default_query() {
			Ok(explanation) => Ok(Resp::new(ExplainDefaultQueryResp {
				explanation: explanation
					.unwrap_or_else(|| "No default query explanation provided".to_owned()),
			})),
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

	async fn get_query_schemas(
		&self,
		_req: Req<GetQuerySchemasReq>,
	) -> QueryResult<Resp<Self::GetQuerySchemasStream>> {
		// Ignore the input, it's empty.
		let query_schemas = self.plugin.schemas().collect::<Vec<QuerySchema>>();
		// TODO: does this need to be configurable?
		let (tx, rx) = mpsc::channel(10);
		tokio::spawn(async move {
			for x in query_schemas {
				let input_schema = serde_json::to_string(&x.input_schema);
				let output_schema = serde_json::to_string(&x.output_schema);

				let schema_resp = match (input_schema, output_schema) {
					(Ok(input_schema), Ok(output_schema)) => Ok(GetQuerySchemasResp {
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

	async fn initiate_query_protocol(
		&self,
		req: Req<Streaming<InitiateQueryProtocolReq>>,
	) -> QueryResult<Resp<Self::InitiateQueryProtocolStream>> {
		let rx = req.into_inner();
		// TODO: - make channel size configurable
		let (tx, out_rx) = match self.curr_host {
			Host::Loopback => mpsc::channel::<QueryResult<InitiateQueryProtocolResp>>(10),
			_ => mpsc::channel::<QueryResult<InitiateQueryProtocolResp>>(100),
		};

		let cloned_plugin = self.plugin.clone();
		let tx_clone = tx.clone();
		tokio::spawn(async move {
			let mut channel = HcSessionSocket::new(tx, rx);
			if let Err(e) = channel.run(cloned_plugin).await {
				error!("Channel error: {e}");
				if !tx_clone.is_closed() {
					if let Err(send_err) = tx_clone
						.send(Err(tonic::Status::internal(format!("Session error: {e}"))))
						.await
					{
						error!("Failed to send error through channel: {send_err}");
					}
				}
			}
		});

		Ok(Resp::new(RecvStream::new(out_rx)))
	}
}
