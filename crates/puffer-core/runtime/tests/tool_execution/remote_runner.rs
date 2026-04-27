use puffer_tool_runner::ToolRunnerService;
use std::sync::mpsc;
use std::thread;
use tokio::runtime::Builder;
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;

/// Owns one background gRPC tool-runner used by runtime integration tests.
pub(super) struct RemoteToolRunnerHandle {
    endpoint: String,
    token: String,
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl RemoteToolRunnerHandle {
    /// Returns the runner endpoint that local clients should connect to.
    pub(super) fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Returns the bearer token expected by the test runner instance.
    pub(super) fn token(&self) -> &str {
        &self.token
    }
}

impl Drop for RemoteToolRunnerHandle {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            if std::thread::panicking() {
                let _ = thread.join();
            } else {
                thread.join().expect("join remote tool runner thread");
            }
        }
    }
}

/// Starts a real gRPC tool-runner on an ephemeral localhost port for one test.
pub(super) fn spawn_remote_tool_runner() -> RemoteToolRunnerHandle {
    let token = "test-remote-tool-runner-token".to_string();
    let service = ToolRunnerService::new(token.clone());
    let (ready_tx, ready_rx) = mpsc::channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let thread = thread::spawn(move || {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime for remote tool runner");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind remote tool runner listener");
            let addr = listener
                .local_addr()
                .expect("read remote tool runner local address");
            ready_tx
                .send(format!("http://{addr}"))
                .expect("publish remote tool runner endpoint");
            tonic::transport::Server::builder()
                .add_service(service.into_service())
                .serve_with_incoming_shutdown(TcpListenerStream::new(listener), async {
                    let _ = shutdown_rx.await;
                })
                .await
                .expect("serve remote tool runner");
        });
    });
    let endpoint = ready_rx
        .recv()
        .expect("receive remote tool runner endpoint");
    RemoteToolRunnerHandle {
        endpoint,
        token,
        shutdown: Some(shutdown_tx),
        thread: Some(thread),
    }
}
