use anyhow::Result;
use nv_common::config::Config;
use reqwest::header::USER_AGENT;
use serde::Deserialize;

// Status struct to deserialize the JSON response from the health endpoint.
#[derive(Deserialize, Debug)]
pub struct Status {
    status: String,
}

pub fn command() -> clap::Command {
    clap::Command::new("health").about("Check the health of the API")
}

pub fn get_health_status(client: &reqwest::blocking::Client, request_url: &str) -> Result<Status> {
    //println!("Fetching health status from {}... ", &request_url);
    match client.get(request_url).header(USER_AGENT, "nvdb").send() {
        Ok(response) => {
            //println!("{:?}: ", &response);
            if response.status().is_success() {
                let status: Status = response.json()?;
                Ok(status)
            } else {
                Err(anyhow::anyhow!(
                    "{} returned an error: {} - {}",
                    &request_url,
                    response.status(),
                    response.text()?
                ))
            }
        }
        Err(err) => {
            if err.is_timeout() {
                Err(anyhow::anyhow!("The request timed out."))
            } else if err.is_connect() {
                Err(anyhow::anyhow!("Failed to connect to the server."))
            } else {
                Err(anyhow::anyhow!("Network error: {err}"))
            }
        }
    }
}

/// Get a response from the REST API health endpoint and print it to stdout.
/// If the API is not reachable, print error message instead.
pub fn run(config: &Config) -> Result<()> {
    let request_url = format!("http://{}/health", config.server_address.clone());
    let client = reqwest::blocking::Client::new();
    match get_health_status(&client, &request_url) {
        Ok(res) => {
            println!("{}: {:?}", &request_url, res.status);
        }
        Err(e) => {
            println!("Error fetching data: {e}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::cmd::api::health::get_health_status;
    use httpmock::prelude::*;

    #[test]
    fn health_check() {
        // Start a lightweight mock server.
        let server = MockServer::start();
        // Create a mock on the server.
        let _mock = server.mock(|when, then| {
            when.method(GET).path("/health");
            then.status(200)
                .header("content-type", "application/json")
                .body(r#"{"status": "ok"}"#);
        });

        // Send an HTTP request to the mock server.
        let client = reqwest::blocking::Client::new();
        let request_url = server.url("/health");
        let status = get_health_status(&client, &request_url).unwrap();
        let body = format!("{:?}", status.status);

        // Assert OK in response
        assert!(body.contains("ok"));
    }
    #[test]
    fn health_check_fails() {
        // Start a lightweight mock server.
        let server = MockServer::start();
        // Create a mock on the server.
        let _mock = server.mock(|when, then| {
            when.method(GET).path("/health");
            then.status(500)
                .header("content-type", "application/json")
                .body(r#"{"status": "error"}"#);
        });

        // Send an HTTP request to the mock server.
        let client = reqwest::blocking::Client::new();
        let request_url = server.url("/health");
        match get_health_status(&client, &request_url) {
            Ok(_res) => {}
            Err(e) => {
                let body = format!("{:?}", e.to_string());
                assert!(body.contains("error"));
            }
        }
    }
}
