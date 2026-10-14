# Security Policy

Lumora / StellarAid takes the security of our smart contracts, SDK, and infrastructure seriously. We welcome responsible security researchers and community members who help us keep our users and on-chain funds secure.

---

## Supported Versions

Security updates and vulnerability patches are actively maintained for the following versions:

| Version / Target | Supported          | Status |
| ---------------- | ------------------ | ------ |
| `main` (trunk)   | :white_check_mark: | Actively maintained, highest priority |
| Latest Release   | :white_check_mark: | Supported for critical security fixes |
| Historical Tags  | :x:                | Deprecated; users are advised to upgrade to latest |

---

## Reporting a Vulnerability

Please **do not** report security vulnerabilities through public GitHub issues, discussions, or pull requests. Public disclosure risks user funds before a mitigation can be deployed.

### Preferred Channel: GitHub Private Vulnerability Reporting
We strongly encourage reporting via GitHub's native **Private Vulnerability Reporting**:
1. Navigate to the repository's **Security** tab on GitHub (`https://github.com/Dfunder/stellarAid-contract/security`).
2. Click on **Advisories** -> **Report a vulnerability**.
3. Fill out the report details, severity rating, and reproducible proof-of-concept steps.

### Alternative Channel: Security Contact Email
If you are unable to use GitHub Private Vulnerability Reporting, you may email our security response team:
- **Email:** `security@stellar-aid.org`
- **Fallback / General Maintainers:** `maintainers@stellar-aid.org`

### What to Include in Your Report
To help us quickly evaluate and remediate the issue, please include:
- A clear description of the vulnerability and its potential impact.
- Affected smart contract(s), SDK modules, or configuration files.
- Step-by-step instructions or an automated test / script to reproduce the issue (e.g., Soroban environment simulation or unit test).
- Any proposed remediation or code patches, if available.

---

## Response Expectations

We commit to a timely and transparent coordination process:
- **Initial Acknowledgement:** Within **48 hours** of receiving your report.
- **Triage & Validation:** Within **5 business days**, confirming whether the issue is reproducible and determining severity.
- **Remediation & Patching:** Timeline depends on severity. Critical on-chain contract vulnerabilities receive immediate prioritization and emergency deployment procedures if necessary.
- **Public Disclosure:** Coordinated disclosure will occur only after a fix is thoroughly validated, deployed to mainnet/testnet, and downstream integrations are safe.

---

## Scope

### In Scope
- Soroban smart contracts (`contracts/campaign`, `contracts/donation`, `contracts/escrow`, `contracts/factory`, `contracts/platform_config`, `contracts/subscription`, `contracts/withdrawal`).
- StellarAid / Lumora SDK packages (`sdk/`, `@stellaraid/wallets`, TypeScript bindings).
- Off-chain worker and relayer services managing transaction submission or event indexing.
- Reentrancy, storage collision, unauthorized state mutation, fund-drain, or authorization bypass vulnerabilities.

### Out of Scope
- Denial-of-Service attacks targeting third-party public Stellar RPC nodes or Friendbot infrastructure.
- Social engineering, phishing, or physical attacks against maintainers or contributors.
- Issues in external dependencies unless accompanied by a working exploit affecting this repository.
- Automated vulnerability scanner or linter dumps without a demonstrable, reproducible attack vector.
- Theoretical vulnerabilities on non-supported testnet mock contracts.

---

## Safe Harbour & Responsible Research

We consider security research conducted under this policy to be authorized. We commit not to pursue legal action against researchers who:
- Make a good-faith effort to avoid privacy violations, data destruction, and interruption or degradation of services.
- Only interact with test accounts, local mock environments, or designated testnets (Testnet / Futurenet) and never disrupt mainnet funds or live user accounts.
- Give us reasonable time to investigate and resolve an issue before making any public disclosure.
- Comply with all applicable coordinated vulnerability disclosure laws and guidelines.

---

## Related Security Resources

For internal operational standards, audit checklists, and emergency protocols, consult:
- [Operational Runbook](docs/OPERATIONAL_RUNBOOK.md)
- [Emergency Procedures](docs/EMERGENCY_PROCEDURES.md)
- [Security Audit Checklist](docs/SECURITY_AUDIT_CHECKLIST.md)
- [Security Review Checklist](docs/SECURITY_REVIEW_CHECKLIST.md)

*Note: For broader project governance, please refer to the project [Code of Conduct](CODE_OF_CONDUCT.md) and [License](LICENSE).*
