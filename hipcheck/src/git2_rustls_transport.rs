//! Custom lib[git2] transport that uses [rustls] over the operating systems's transport.
//!
//! This should accurately implement the spec described at
/// <https://www.git-scm.com/docs/http-protocol>.

use std::{io::{self, Read, Write, Error as IoError}, sync::{Arc, Once, OnceLock}};
use base64::Engine;
use dialoguer::{Input, Password};
use git2::{transport::{self, Service, SmartSubtransport, SmartSubtransportStream, Transport}, Error as Git2Error, Remote};
use http::Method;
use rustls::{ClientConfig, RootCertStore};
use ureq::{Agent, AgentBuilder, Request, Error as UreqError};
use crate::shell::Shell;
use base64::prelude::BASE64_STANDARD;

/// A global static [Once] to make sure we don't register the custom transport more than once.
static REGISTER: Once = Once::new();

/// The agent used by the custom transport that includes system certs.
static AGENT: OnceLock<Agent> = OnceLock::new();

/// Register this transport with lib[git2].
pub fn register() {
    // SAFETY: This function needs to be synchronized with creations of transports.
    // Since we only create 1 custom transport, this should be safe.
    REGISTER.call_once(|| unsafe {
        log::debug!("Registering custom rustls based transport for http(s) prefixes");
        // Note that we have to use prefixes without the `://` at the end of them otherwise libgit2 will get confused and 
        // ignore/not use them.
        transport::register("https", make_transport).unwrap();
        transport::register("http", make_transport).unwrap();
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

// The following 3 functions use info from 
// https://github.com/libgit2/libgit2/blob/2ecc8586f7eec4063b5da1563d0a33f9e9f9fcf7/src/libgit2/transports/http.c#L68-L95

/// Get the url path for a [Service].
const fn service_url_path(s: Service) -> &'static str {
    match s {
        Service::UploadPackLs => "/info/refs?service=git-upload-pack",
        Service::UploadPack => "/git-upload-pack",
        Service::ReceivePackLs => "/info/refs?service=git-receive-pack",
        Service::ReceivePack => "/git-receive-pack",
    }
}

/// Get the [http] method for a [Service].
const fn service_method(s: Service) -> Method {
    match s {
        Service::ReceivePack | Service::UploadPack => Method::POST,
        Service::ReceivePackLs | Service::UploadPackLs => Method::GET,
    }
}

const fn service_request_type(s: Service) -> Option<&'static str> {
    match s {
        Service::UploadPackLs | Service::ReceivePackLs => None,
        Service::ReceivePack => Some("application/x-git-receive-pack-request"),
        Service::UploadPack => Some("application/x-git-upload-pack-request"),
    }
}

const fn service_response_type(s: Service) -> &'static str {
    match s {
        Service::UploadPackLs => "application/x-git-upload-pack-advertisement",
        Service::ReceivePackLs => "application/x-git-receive-pack-advertisement",
        Service::UploadPack => "application/x-git-upload-pack-result",
        Service::ReceivePack => "application/x-git-receive-pack-result",
    }
}

/// Function used as a factory
fn make_transport(remote: &Remote) -> Result<Transport, Git2Error> {
    Transport::smart(remote, true, CustomTransport)
}

struct CustomTransport;

struct CustomSubtransportStream {
    /// The request with the proper headers/options. 
    req: Request,
    req_body: Vec<u8>,
    response_reader: Option<Box<dyn Read + Send + Sync + 'static>>
}

impl CustomSubtransportStream {
    /// Send the request and allow basic http authentication if needed.
    fn send_req_allow_auth(&mut self) -> io::Result<()> {
        // Get a reference to the request body.
        let body: &[u8] = &self.req_body;
        
        // Clone the request and send with that body.
        // Depending on the response, we may resend with auth. 
        match self.req.clone().send_bytes(body) {
            // If we get a response successfully, update self and return ok.
            Ok(response) => {
                self.response_reader = Some(response.into_reader());
                Ok(())
            }

            // If we get a response indicating the need for basic auth, handle basic auth.
            Err(UreqError::Status(401, response)) => {
                // Check if it really is an issue of basic auth.
                let response_wants_basic_auth = response
                    .header("WWW-Authenticate")
                    .is_some_and(|header_val| header_val.starts_with("Basic "));

                if !response_wants_basic_auth {
                    return Err(IoError::other(UreqError::Status(401, response)));
                }

                // Get the username and password from the user (suspend progress bars to do this).
                let (user, pass) = Shell::in_suspend(|| {
                    // Get the remote url that is asking for auth.
                    let remote_url = response.get_url();
    
                    println!("Git remote ({remote_url}) is requesting authentication.");
    
                    let user: String = Input::new()
                        .with_prompt(format!("Git remote ({remote_url}) username:"))
                        .interact_text()
                        .expect("read user input");
    
                    let pass = Password::new()
                        .with_prompt(format!("Git remote ({remote_url}) password:"))
                        .interact()
                        .expect("read user password");
    
                    (user, pass)
                });
    
                // Encode the username and password concatted with a colon.
                let encoded = BASE64_STANDARD.encode(format!("{user}:{pass}"));
    
                // Add the header to the request and resend.
                let new_req = self.req
                    .clone()
                    .set("Authorization", &format!("Basic: {encoded}"));

                match new_req.send_bytes(body) {
                    Ok(response) => {
                        self.response_reader = Some(response.into_reader());
                        Ok(())
                    },

                    Err(err) => Err(IoError::other(err))
                }
            }

            // Other errors that aren't asking for authentication.
            Err(ureq_err) => Err(IoError::other(ureq_err))
        }
    }
}

impl Read for CustomSubtransportStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // If we read and there is no response yet, send the request. 
        if self.response_reader.is_none() {
            self.send_req_allow_auth()?;
        }

        // Unwrap here is fine, since sending the request should have instantiated the response reader
        // or errored. 
        self.response_reader.as_mut().unwrap().read(buf)
    }
}

impl Write for CustomSubtransportStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.req_body.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.req_body.flush()
    }
}

impl SmartSubtransport for CustomTransport {
    fn action(&self, url: &str, action: Service) -> Result<Box<dyn SmartSubtransportStream>, Git2Error> {
        // Get the URL to send the request to.
        let url = format!("{url}{}", service_url_path(action));

        // Get the agent with rustls native certs to create an http request.
        let mut req = agent()
            .request(service_method(action).as_str(), url.as_str())
            .set("Accept", service_response_type(action));

        // Set the request type if neccessary. 
        if let Some(req_type) = service_request_type(action) {
            req = req.set("Content-Type", req_type);
        }

        // Create the custom stream and return.
        let stream = CustomSubtransportStream {
            req,
            req_body: Vec::new(),
            response_reader: None,
        };

        Ok(Box::new(stream))
    }

    fn close(&self) -> Result<(), Git2Error> {
        // Close is always just OK since this struct has no state and connections will handle themselves 
        // via drop (as long as git2 handles that properly). 
        Ok(())
    }
}
