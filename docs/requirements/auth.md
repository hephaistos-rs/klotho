# Authentication requirements

**Scope:** establishing *who* is making a request. That covers accounts, passwords, magic links, passkeys, tokens, SSH keys, external identity providers (SSO), two-factor authentication, sessions and account registration.

How these fit together as concrete flows and endpoints is described in [design/auth-flows.md](../design/auth-flows.md).

**Not in scope:** *what* an authenticated identity is allowed to do ([access-control.md](access-control.md)) and platform-wide protections like TLS and rate limits ([security.md](security.md)).

## Accounts and registration

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-AUTH-001 | Klotho **must** support local user accounts identified by username and email address. | All four. | must-have |
| FR-AUTH-002 | Administrators **must** be able to set registration to open, invite-only or disabled. | GitLab and Gitea/Forgejo. Most self-hosted instances are private. | must-have |
| FR-AUTH-003 | A fresh install **must** give a way to create the first administrator without leaving an unauthenticated admin endpoint open, for example a one-time setup token or a CLI command. | Gitea's install page and GitLab's initial root password. Instances exposed during setup get taken over. | must-have |
| FR-AUTH-004 | Users **should** be able to verify their email address, and administrators **should** be able to require verification before first use. | All four. | should-have |
| FR-AUTH-005 | Users **must** be able to reset a forgotten password through a single-use, time-limited link sent to a verified email address. Administrators **must** be able to reset any password through the CLI. | All four offer email resets. A CLI reset (Gitea `admin user change-password`) covers instances without SMTP. | must-have |
| FR-AUTH-006 | Users **should** be able to sign in with a magic link sent to a verified email address. The link **must** be single-use, expire within 15 minutes and be stored only as a hash. Asking for a link **must** return the same response whether or not the address exists. Following the link **must** need a click on a confirmation button, not just a page load. The link **must not** skip two-factor authentication. Magic links **must** be unavailable when SMTP isn't configured. | None of the four forges offer this; it's our own choice, common in modern web apps. The confirmation click matters because corporate mail scanners (e.g. Outlook Safe Links) open links in emails and would use up a single-use token that works on page load. | should-have |

## Credentials

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-AUTH-010 | Passwords **must** be stored only as salted hashes from a memory-hard algorithm (Argon2id, or scrypt/bcrypt as a fallback). | Gitea uses Argon2/PBKDF2/scrypt/bcrypt; OWASP recommends Argon2id. | must-have |
| FR-AUTH-011 | Users **must** be able to create personal access tokens with a name, a set of scopes and an optional expiry date, and **must** be able to revoke them. | GitHub fine-grained PATs, GitLab and Gitea tokens. How scopes limit access is set in FR-ACL-030. | must-have |
| FR-AUTH-012 | A token **must** be shown in full only once, when it is created, and **must** be stored only as a hash. | GitHub, GitLab and Gitea never show a token again. Hashing stops a database leak from exposing working tokens. | must-have |
| FR-AUTH-013 | Tokens **should** carry a recognisable prefix, e.g. `klotho_pat_…`, so secret scanners can detect them. | GitHub (`ghp_`), GitLab (`glpat-`). Leaked tokens get found and revoked sooner. | should-have |
| FR-AUTH-014 | Users **must** be able to register SSH public keys (Ed25519, ECDSA, RSA ≥ 3072 bits). A key fingerprint **must** belong to at most one account or deploy key. | All four. On SSH the key is the identity, so a shared key would be ambiguous. | must-have |
| FR-AUTH-015 | Repository administrators **should** be able to add deploy keys: SSH keys tied to a single repository, read-only unless write access is explicitly granted. | All four. Lets CI or servers pull one repository without a user account. | should-have |
| FR-AUTH-016 | Users **should** be able to sign in with a passkey alone (a discoverable WebAuthn credential), with no password, and **may** remove their password entirely once a passkey is registered. | GitHub supports passwordless passkey sign-in. Passkeys resist phishing, and removing the password removes the most commonly attacked credential. | should-have |

## Git and API authentication

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-AUTH-020 | Git over HTTP **must** accept HTTP Basic authentication with a username and a personal access token. | All four. It works with git's credential helpers. | must-have |
| FR-AUTH-021 | Git over HTTP **should** reject account passwords by default, accepting only tokens. Administrators **may** re-enable passwords for accounts without two-factor authentication. | GitHub removed password auth for git in 2021. GitLab requires tokens once 2FA is on. Passwords sent on every fetch are a big target. | should-have |
| FR-AUTH-022 | The API **must** accept `Authorization: Bearer <token>`. | Standard; GitHub, GitLab and Gitea accept it. | must-have |
| FR-AUTH-023 | Requests without credentials to a resource that needs them **must** get `401` with a `WWW-Authenticate: Basic` challenge on git HTTP paths, so git prompts for credentials. | Git's smart HTTP client only asks for credentials after a 401 challenge. | must-have |
| FR-AUTH-024 | Klotho **may** support the OAuth 2.0 device authorization grant (RFC 8628), so command-line tools can sign in through the browser without handling passwords. | GitHub's `gh auth login` uses it. It fits a future `klotho` CLI and git credential helpers. | nice-to-have |

## External identity and 2FA

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-AUTH-030 | Klotho **should** support sign-in through external OpenID Connect providers, linking to existing accounts by verified email or on explicit user confirmation. | GitLab, Gitea and Forgejo all support OIDC. Self-hosters often already run Keycloak or Authentik. | should-have |
| FR-AUTH-031 | Klotho **may** support LDAP authentication. | GitLab and Gitea. Common in enterprises. | nice-to-have |
| FR-AUTH-032 | Users **should** be able to enable TOTP two-factor authentication, with single-use recovery codes. | All four. | should-have |
| FR-AUTH-033 | Users **should** be able to register WebAuthn/passkey authenticators as a second factor. | All four support WebAuthn. It resists phishing. | should-have |
| FR-AUTH-034 | Administrators **may** require two-factor authentication for all users or for members of specific organisations. | GitHub (org requirement), GitLab (instance and group enforcement). | nice-to-have |
| FR-AUTH-035 | Klotho **should** be an OAuth2/OIDC provider, so Lachesis, Atropos and third-party tools can sign users in and act for them. | Gitea/Forgejo and GitLab are OAuth2 providers, which is how Woodpecker CI and similar tools sign in to them. It lets the three Moirai share one identity. | should-have |
| FR-AUTH-036 | An external identity **must** be identified by its issuer and subject (`iss` + `sub`), never by email address alone. Signing in with a new external identity **may** link it to an existing account by email only when the provider marks the email as verified **and** the administrator has enabled email linking for that provider. Otherwise the user must link it while signed in. | Linking by email alone allows account takeover: the 2023 "nOAuth" issue with Microsoft Entra ID let attackers set any email on their own identity. | must-have |
| FR-AUTH-037 | OIDC sign-in **must** use the authorization code flow with PKCE and **must** check `state` and `nonce`. | The OAuth 2.0 Security Best Current Practice (RFC 9700) recommends it. It prevents code interception, CSRF on the callback and token replay. | must-have |
| FR-AUTH-038 | Klotho **may** support SAML 2.0 sign-in. Until it does, the documentation **should** explain how to connect SAML providers through an OIDC bridge such as Keycloak or Authentik. | GitLab supports SAML natively. The Rust SAML library (`samael`, version 0.0.x) is immature, and a bridge gives enterprises SAML without Klotho parsing XML signatures, a common source of vulnerabilities. | nice-to-have |
| FR-AUTH-039 | Klotho **may** accept SCIM 2.0 provisioning from an identity provider, to create, update and deactivate accounts automatically. | GitHub Enterprise and GitLab support SCIM. Accounts are then deactivated as soon as someone leaves the organisation. | nice-to-have |

## Sessions

| ID | Requirement | Rationale | Priority |
|---|---|---|---|
| FR-AUTH-040 | Web sessions **must** use cookies marked `HttpOnly`, `Secure` (when served over HTTPS) and `SameSite=Lax` or stricter. | Standard practice. | must-have |
| FR-AUTH-041 | Sessions **must** expire after a configurable idle period and a configurable absolute lifetime. | All four. | must-have |
| FR-AUTH-042 | Users **should** be able to see their active sessions and revoke any or all of them. Changing a password **must** revoke all other sessions. | GitHub and GitLab list sessions. Revoking on password change limits the damage after a compromise. | should-have |
| FR-AUTH-043 | Repeated failed sign-in attempts for one account or source address **must** be throttled, with delays that grow on each failure or a temporary lockout. | GitLab locks accounts after 10 failures. This stops online password guessing. | must-have |
| FR-AUTH-044 | Sensitive actions **should** require the user to have re-authenticated within the last 15 minutes ("sudo mode"), even with a valid session. The actions are: changing email, password or 2FA settings, creating tokens or keys, deleting or transferring repositories, and changing permissions. | GitHub sudo mode and GitLab admin mode. A stolen session cookie can't then take over the account or destroy data. | should-have |
| FR-AUTH-045 | The session ID **must** be regenerated at sign-in and at every privilege change, and the session cookie **should** use the `__Host-` prefix. | Prevents session fixation. `__Host-` tells the browser the cookie can only be set by this exact host over HTTPS, with no subdomain or path tricks. | must-have |

## Conflicts with the current implementation

Checked against commit `6b4895f`.

| Requirement | Current behaviour | Where |
|---|---|---|
| FR-AUTH-001, FR-AUTH-020, FR-AUTH-022 (any authentication at all) | **Conflict (critical).** There are no accounts and no authentication. Every git and API request is anonymous, and anonymous callers can push (acknowledged in a TODO). | [git_http.rs:9](../../crates/server/src/git_http.rs#L9), [lib.rs:10-15](../../crates/server/src/lib.rs#L10-L15) |

Every other requirement in this file is unimplemented, not in conflict.
