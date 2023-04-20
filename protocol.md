## Twilght Remote Desktop Protocol Draft

### Intro
This is the description of protocol used in Twilight Remote Desktop.

### Goals
 * Transport that looks like normal HTTP(S)
 * Support for any combination of forward- and reverse- proxy,
including any off-the-shelf HTTP proxy like nginx.
 * Ability to send WoL signal from reverse proxy
 * Optional UDP support
 * Flexible enough to support the web browsers
 * No need to use HTTP after switching to WebSocket

### Intro
The default port is 6498, which is for TLS.

Upon connecting, the client will act like an HTTP client.
Then it will switch to websocket and begin communicating using
flatbuffer protocol.

### Encryption
The protocol trusts the outer TLS for doing encryption.
If plain HTTP is used, the whole connection will not be encrypted.

If TLS is used but the certificate is not trusted (e.g. self-signed),
server auth will be performed.

### HTTP Endpoints
Note: The default prefix for the HTTP endpoints is `/twilight`,
which is configurable. For example, `/twilight/auth`.

#### Non-privileged endpoints
Endpoints described here may be called before client auth.

---
`POST /auth-server?type=???`
Authenticate the serve with specified type.

Since TLS is not yet implemented, this endpoint is not designed yet.

---
`POST /auth?type=????`
Authenticate the client with specified type.

200 &rarr; Successfully authorized. Client may proceed.  
Others &rarr; Returns message as body (Unresolved question: how to localize them?)

A successful response will set a cookie 🍪 to authenticate requests.

---
`POST /auth?type=debug`
Always accept the client.

Client will send its username in the request body.
It is trimmed before use.

This auth type is insecure and is mainly for debugging.

---

#### Privileged endpoints
Endpoints described here needs the authentication cookie 🍪,
or they will return 403 Forbidden.

---
`GET /stream?version=1`
Start WebSocket connection.

It upgrades the underlying connection into the WebSocket connection.
May return error if version is unsupported.


### WebSocket endpoint v1
(WIP)

Every message is `[u16le: stream id][bytes: flatbuffer data]` concatenated.
Stream ID 0 is control (`ControlFrame`).
Other channels are dynamically allocated.
