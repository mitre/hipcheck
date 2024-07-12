//! Custom lib[git2] transport that uses [rustls] over the operating systems's transport. 
//! 
//! This should accurately implement the spec described at
/// <https://www.git-scm.com/docs/http-protocol>.

use std::sync::{Arc, Once, OnceLock};

use git2::{transport::{self, Service, SmartSubtransport, SmartSubtransportStream, Transport}, Error, Remote};
use rustls::{ClientConfig, RootCertStore};
use ureq::{Agent, AgentBuilder};

/// A global static [Once] to make sure we don't register the custom transport more than once. 
static REGISTER: Once = Once::new();

/// The agent used by the custom transport that includes system certs. 
static AGENT: OnceLock<Agent> = OnceLock::new();

/// Register this transport with lib[git2]. 
pub fn register() {
    // SAFETY: This function needs to be synchronized with creations of transports.
    // Since we only create 1 custom transport, this should be safe. 
    REGISTER.call_once(|| unsafe {
        transport::register("https://", make_transport).unwrap();
        transport::register("http://", make_transport).unwrap();
    })
}

// TODO: maybe replace [crate::http::tls::new_agent] with this function. 
/// Get or initialize the global static agent used in the git2 custom transport.
/// This is similar to [crate::http::tls::new_agent] but it uses a global static
/// and is more forgiving to invalid system native certs.
fn agent() -> &'static Agent {
    AGENT.get_or_init(|| {
        // Retrieve system certs
        let mut roots = RootCertStore::empty();
        let native_certs = rustls_native_certs::load_native_certs().expect("loaded native certs");
        roots.add_parsable_certificates(native_certs.into_iter());

        // Add certs to connection configuration
        let tls_config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();

        // Construct agent
        AgentBuilder::new().tls_config(Arc::new(tls_config)).build()
    })
}

/// Function used as a factory 
fn make_transport(remote: &Remote) -> Result<Transport, Error> {
    Transport::smart(remote, true, CustomTransport)
}

struct CustomTransport;

struct CustomSubtransport;

impl SmartSubtransport for CustomTransport {
    fn action(&self, url: &str, action: Service) -> Result<Box<dyn SmartSubtransportStream>, Error> {
        dbg!(url);

        // Get the path to use for the given service. 
        // This match block is pulled from https://docs.rs/git2-curl/latest/src/git2_curl/lib.rs.html#103. 
        let (service, path, method) = match action {
            Service::UploadPackLs => ("upload-pack", "/info/refs?service=git-upload-pack", "GET"),
            Service::UploadPack => ("upload-pack", "/git-upload-pack", "POST"),
            Service::ReceivePackLs => ("receive-pack", "/info/refs?service=git-receive-pack", "GET"),
            Service::ReceivePack => ("receive-pack", "/git-receive-pack", "POST"),
        };

        dbg!(service, path, method);

        
        unimplemented!()
    }

    fn close(&self) -> Result<(), Error> {
        todo!()
    }
}
