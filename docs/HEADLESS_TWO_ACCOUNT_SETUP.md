# Two accounts on a Linux server

Follow [Linux server login](server-login.md) to install explicit display
prerequisites, start `jailgun setup --server-browser --no-open`, and pair the
dashboard through an SSH forward. The daemon manages the displays and private
viewer transport. No separately launched VNC server, websockify process or CDP
forward is needed for concept workflows.

In **Accounts**, connect the first account, complete normal ChatGPT login in
**Open login view**, and confirm the observed identity and model. Repeat for the
second account using its own email. Each registration keeps a separate profile,
browser and X authority cookie. Select the desired account in **New concept**.
Use the actual IDs returned by `jailgun accounts list` for CLI or MCP requests;
IDs in examples are not substitutes for registered accounts.

```bash
jailgun accounts list --json
jailgun brainstorm "A neighborhood lending library" --account YOUR_ACCOUNT_ID --tabs 5
```

Restart with `jailgun serve --server-browser` to reuse the private profiles.
Jailgun observes authentication again before starting submissions. If the
provider expires a session, reconnect that account from the dashboard.
Completed results remain available. MCP reports the account's required action;
account login remains an operator dashboard operation. See [MCP](mcp.md).

Keep the entire runtime private. Do not remove Chrome profile locks after a
startup failure: another browser may own that profile. Inspect the owning
process and use a normal shutdown before retrying. Do not run the advanced
archive scheduler against profiles owned by the concept daemon.

Automated tests verify display-cookie isolation, distinct displays and scoped
browser operations. Live acceptance with two operator-owned ChatGPT accounts
is still pending; persistent login is not a promise that the provider will
never request verification again.
