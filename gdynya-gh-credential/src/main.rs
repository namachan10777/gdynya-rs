use cargo_credential::{Action, CacheControl, Credential, CredentialResponse};

struct GhToken;

impl Credential for GhToken {
    fn perform(
        &self,
        _: &cargo_credential::RegistryInfo,
        action: &Action,
        _: &[&str],
    ) -> Result<CredentialResponse, cargo_credential::Error> {
        match action {
            Action::Unknown => Ok(CredentialResponse::Unknown),
            Action::Get(_) => {
                let output = std::process::Command::new("gh")
                    .arg("auth")
                    .arg("token")
                    .output()
                    .map_err(|e| cargo_credential::Error::Other(Box::new(e)))?;
                Ok(CredentialResponse::Get {
                    token: String::from_utf8(output.stdout)
                        .map_err(|e| cargo_credential::Error::Other(Box::new(e)))?
                        .trim()
                        .to_string()
                        .into(),
                    cache: CacheControl::Session,
                    operation_independent: true,
                })
            }
            Action::Login(_) => Ok(CredentialResponse::Login),
            Action::Logout => Ok(CredentialResponse::Logout),
            _ => Err(cargo_credential::Error::Unknown),
        }
    }
}

fn main() {
    cargo_credential::main(GhToken)
}
