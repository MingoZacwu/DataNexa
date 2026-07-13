use keyring::Entry;
use zeroize::Zeroizing;

#[derive(Clone)]
pub struct CredentialVault {
    service: String,
}

impl CredentialVault {
    pub fn new() -> Self {
        Self {
            service: "com.mingo.datanexa".to_string(),
        }
    }

    pub fn credential_ref(connection_id: &str) -> String {
        format!("vault://{connection_id}")
    }

    pub fn put(&self, credential_ref: &str, password: String) -> anyhow::Result<()> {
        self.put_secret(credential_ref, Zeroizing::new(password))
    }

    pub fn put_secret(
        &self,
        credential_ref: &str,
        password: Zeroizing<String>,
    ) -> anyhow::Result<()> {
        let account = account_from_ref(credential_ref)?;
        let entry = Entry::new(&self.service, &account)?;
        entry.set_password(password.as_str())?;

        match self.get(credential_ref)? {
            Some(saved_password) if !saved_password.is_empty() => {}
            Some(_) => {
                return Err(anyhow::anyhow!(
                    "credential was saved but read back as empty; secure storage is not usable"
                ));
            }
            None => {
                return Err(anyhow::anyhow!(
                    "credential was saved but could not be read back; secure storage is not usable"
                ));
            }
        }
        Ok(())
    }

    pub fn get(&self, credential_ref: &str) -> anyhow::Result<Option<Zeroizing<String>>> {
        let account = account_from_ref(credential_ref)?;
        let entry = Entry::new(&self.service, &account)?;
        match entry.get_password() {
            Ok(password) => Ok(Some(Zeroizing::new(password))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn delete(&self, credential_ref: &str) -> anyhow::Result<()> {
        let account = account_from_ref(credential_ref)?;
        let entry = Entry::new(&self.service, &account)?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

fn account_from_ref(credential_ref: &str) -> anyhow::Result<String> {
    credential_ref
        .strip_prefix("vault://")
        .map(|value| value.to_string())
        .ok_or_else(|| anyhow::anyhow!("invalid credential reference"))
}
