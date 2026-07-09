use keyring::Entry;
use zeroize::Zeroize;

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

    pub fn put(&self, credential_ref: &str, mut password: String) -> anyhow::Result<()> {
        let account = account_from_ref(credential_ref)?;
        let entry = Entry::new(&self.service, &account)?;
        let result = entry.set_password(&password);
        password.zeroize();
        result?;

        match self.get(credential_ref)? {
            Some(mut saved_password) if !saved_password.is_empty() => {
                saved_password.zeroize();
            }
            Some(mut saved_password) => {
                saved_password.zeroize();
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

    pub fn get(&self, credential_ref: &str) -> anyhow::Result<Option<String>> {
        let account = account_from_ref(credential_ref)?;
        let entry = Entry::new(&self.service, &account)?;
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
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
