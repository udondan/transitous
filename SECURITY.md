# Security

## Reporting a vulnerability

Please report vulnerabilities privately through
[GitHub security advisories](https://github.com/udondan/transitous/security/advisories/new),
not in a public issue.

## Scope

`transitous` only reads public timetable data from the Transitous API. It
sends no personal data, except the contact you configure, which goes into the
User-Agent of every request as the usage rules of Transitous require.

Text from the API, like stop names and alert texts, is untrusted. The MCP
server marks it as data for the model, never as instructions.

The MCP server runs over stdio by default. Its HTTP transport needs a bearer
token and binds to loopback unless told otherwise.
