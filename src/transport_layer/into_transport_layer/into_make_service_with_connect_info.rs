use ::anyhow::anyhow;
use ::anyhow::Context;
use ::anyhow::Result;
use ::async_trait::async_trait;
use ::axum::extract::connect_info::IntoMakeServiceWithConnectInfo;
use ::axum::serve;
use ::axum::Router;
use ::tokio::net::TcpListener as TokioTcpListener;
use ::tokio::spawn;
use ::url::Url;
use axum::serve::IncomingStream;

use super::IntoTransportLayer;
use crate::internals::HttpTransportLayer;
use crate::transport_layer::TransportLayer;
use crate::transport_layer::TransportLayerBuilder;

#[async_trait]
impl<C> IntoTransportLayer for IntoMakeServiceWithConnectInfo<Router, C>
where
    for<'a> C: axum::extract::connect_info::Connected<IncomingStream<'a>>,
{
    async fn into_http_transport_layer(
        self,
        builder: TransportLayerBuilder,
    ) -> Result<Box<dyn TransportLayer>> {
        let (socket_addr, tcp_listener, maybe_reserved_port) =
            builder.tcp_listener_with_reserved_port()?;
        tcp_listener.set_nonblocking(true)?;
        let tokio_tcp_listener = TokioTcpListener::from_std(tcp_listener)?;

        let server_handle = spawn(async move {
            let server = serve(tokio_tcp_listener, self)
                .await
                .with_context(|| "Failed to create ::axum::Server for TestServer")
                .expect("Expect server to start serving");
        });

        let server_address = format!("http://{socket_addr}");
        let server_url: Url = server_address.parse()?;

        Ok(Box::new(HttpTransportLayer::new(
            server_handle,
            maybe_reserved_port,
            server_url,
        )))
    }

    async fn into_mock_transport_layer(self) -> Result<Box<dyn TransportLayer>> {
        Err(anyhow!("`IntoMakeServiceWithConnectInfo` cannot be mocked, as it's underlying implementation requires a real connection. Set the `TestServerConfig` to run with a transport of `HttpRandomPort`, or a `HttpIpPort`."))
    }

    async fn into_default_transport(
        self,
        builder: TransportLayerBuilder,
    ) -> Result<Box<dyn TransportLayer>> {
        self.into_http_transport_layer(builder).await
    }
}

#[cfg(test)]
mod test_into_http_transport_layer_for_into_make_service_with_connect_info {
    use ::axum::routing::get;
    use ::axum::Router;
    use ::std::net::SocketAddr;

    use crate::TestServer;
    use crate::TestServerConfig;
    use crate::Transport;

    async fn get_ping() -> &'static str {
        "pong!"
    }

    #[tokio::test]
    async fn it_should_create_and_test_with_make_into_service_with_connect_info() {
        // Build an application with a route.
        let app = Router::new()
            .route("/ping", get(get_ping))
            .into_make_service_with_connect_info::<SocketAddr>();

        // Run the server.
        let config = TestServerConfig {
            transport: Some(Transport::HttpRandomPort),
            ..TestServerConfig::default()
        };
        let server = TestServer::new_with_config(app, config).await.expect("Should create test server");

        // Get the request.
        server.get(&"/ping").await.assert_text(&"pong!");
    }
}

#[cfg(test)]
mod test_into_mock_transport_layer_for_into_make_service_with_connect_info {
    use ::axum::routing::get;
    use ::axum::Router;
    use ::std::net::SocketAddr;

    use crate::TestServer;
    use crate::TestServerConfig;
    use crate::Transport;

    async fn get_ping() -> &'static str {
        "pong!"
    }

    #[tokio::test]
    async fn it_should_panic_when_creating_test_using_mock() {
        // Build an application with a route.
        let app = Router::new()
            .route("/ping", get(get_ping))
            .into_make_service_with_connect_info::<SocketAddr>();

        // Build the server.
        let config = TestServerConfig {
            transport: Some(Transport::MockHttp),
            ..TestServerConfig::default()
        };
        let result = TestServer::new_with_config(app, config).await;
        let err = result.unwrap_err();
        let err_msg = format!("{}", err);

        assert_eq!(err_msg, "`IntoMakeServiceWithConnectInfo` cannot be mocked, as it's underlying implementation requires a real connection. Set the `TestServerConfig` to run with a transport of `HttpRandomPort`, or a `HttpIpPort`.");
    }
}
