#[cfg(not(target_os = "windows"))]
use std::{collections::BTreeSet, fs};

use serde::Serialize;

#[cfg(target_os = "windows")]
use tokio::process::Command;

use crate::ToolError;

#[derive(Debug, Clone, Serialize)]
pub struct AccountPrivilegeSnapshot {
    pub privileged_accounts: Vec<String>,
    pub non_default_privileged_accounts: Vec<String>,
    pub evidence: Vec<String>,
}

pub async fn collect_account_privilege_snapshot() -> Result<AccountPrivilegeSnapshot, ToolError> {
    #[cfg(target_os = "windows")]
    {
        collect_windows_account_snapshot().await
    }

    #[cfg(not(target_os = "windows"))]
    {
        collect_unix_account_snapshot()
    }
}

#[cfg(target_os = "windows")]
async fn collect_windows_account_snapshot() -> Result<AccountPrivilegeSnapshot, ToolError> {
    let output = Command::new("net")
        .args(["localgroup", "administrators"])
        .output()
        .await?;

    if !output.status.success() {
        return Err(ToolError::Execution(format!(
            "net localgroup administrators failed with status {:?}",
            output.status.code()
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let members = parse_windows_admin_members(&stdout);

    let non_default_members = members
        .iter()
        .filter(|member| !is_windows_default_admin_member(member))
        .cloned()
        .collect::<Vec<_>>();

    let evidence = stdout
        .lines()
        .take(80)
        .map(|line| line.to_string())
        .collect();

    Ok(AccountPrivilegeSnapshot {
        privileged_accounts: members,
        non_default_privileged_accounts: non_default_members,
        evidence,
    })
}

#[cfg(target_os = "windows")]
fn parse_windows_admin_members(stdout: &str) -> Vec<String> {
    let mut members = Vec::new();
    let mut in_member_list = false;

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("---") {
            in_member_list = true;
            continue;
        }

        if !in_member_list {
            continue;
        }

        if trimmed.to_ascii_lowercase().contains("command completed") {
            break;
        }

        members.push(trimmed.to_string());
    }

    members.sort_unstable();
    members.dedup();
    members
}

#[cfg(target_os = "windows")]
fn is_windows_default_admin_member(member: &str) -> bool {
    let lower = member.to_ascii_lowercase();
    [
        "administrator",
        "administrators",
        "domain admins",
        "enterprise admins",
        "nt authority\\system",
    ]
    .iter()
    .any(|default_name| lower == *default_name)
}

#[cfg(not(target_os = "windows"))]
fn collect_unix_account_snapshot() -> Result<AccountPrivilegeSnapshot, ToolError> {
    let mut privileged_accounts = BTreeSet::new();
    let mut non_default_accounts = BTreeSet::new();
    let mut evidence = Vec::new();

    let passwd_content = fs::read_to_string("/etc/passwd");
    let group_content = fs::read_to_string("/etc/group");

    if passwd_content.is_err() && group_content.is_err() {
        return Err(ToolError::Execution(
            "unable to read /etc/passwd or /etc/group for account snapshot".to_string(),
        ));
    }

    if let Ok(passwd) = passwd_content {
        for line in passwd.lines() {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }

            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() < 4 {
                continue;
            }

            let account = fields[0].trim();
            let uid = fields[2].trim();
            if uid == "0" {
                let account_name = account.to_string();
                evidence.push(format!("uid=0 account: {account_name}"));
                privileged_accounts.insert(account_name.clone());
                if account_name != "root" {
                    non_default_accounts.insert(account_name);
                }
            }
        }
    }

    if let Ok(group) = group_content {
        for line in group.lines() {
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }

            let fields: Vec<&str> = line.split(':').collect();
            if fields.len() < 4 {
                continue;
            }

            let group_name = fields[0].trim();
            if !matches!(group_name, "sudo" | "wheel" | "adm") {
                continue;
            }

            let members = fields[3]
                .split(',')
                .map(str::trim)
                .filter(|member| !member.is_empty())
                .collect::<Vec<_>>();

            for member in members {
                evidence.push(format!("{group_name} member: {member}"));
                privileged_accounts.insert(member.to_string());
                if member != "root" {
                    non_default_accounts.insert(member.to_string());
                }
            }
        }
    }

    Ok(AccountPrivilegeSnapshot {
        privileged_accounts: privileged_accounts.into_iter().collect(),
        non_default_privileged_accounts: non_default_accounts.into_iter().collect(),
        evidence,
    })
}
