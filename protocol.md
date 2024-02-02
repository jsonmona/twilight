## Twilght Remote Desktop Protocol Draft

### Intro
This is the description of protocol used in Twilight Remote Desktop.

### Goals
 * Transport using HTTP(S) and WebSocket
 * Support for any combination of forward- and reverse- proxy,
including any off-the-shelf HTTP proxy like nginx.
 * Ability to send WoL signal from reverse proxy
 * Optional UDP support
 * Flexible enough to support the web browsers
 * No need to use HTTP after switching to WebSocket

### Connection
If no scheme is given, it defaults to HTTPS through port 1517.

If scheme `twilight` or `twilights` ('s' for TLS) is given, the default port is
1518 and 1517 respectively.

If scheme either `http` or `https` is given, the default port is 80 and 443
respectively.

Upon connecting, the client will act like an HTTP client.
Then it will switch to websocket and begin communicating using
flatbuffer protocol.

### Encryption
The protocol trusts HTTPS for doing encryption.
If plain HTTP is used, the whole connection will not be encrypted.

If TLS is used but the certificate is not trusted (e.g. self-signed),
client may perform a manual auth (by PIN, etc.) and mark the cert as trusted.

### HTTP Endpoints
Note: The default prefix for the HTTP endpoints is `/twilight`,
which is configurable. For example, `/auth` becomes `/twilight/auth`.

#### Non-privileged endpoints
Endpoints described here may be called before client auth.

---
`POST /auth-server?type=???`
Authenticate the server with specified type.

Since TLS is not yet implemented, this endpoint is not designed yet.

---
`GET /auth`
List available authenticate types.

The list may change depending on the client IP.

---
`POST /auth/{type}`
Authenticate the client with specified type.

200 &rarr; Successfully authorized. Client may proceed.  
Others &rarr; Returns message as body (Unresolved question: how to localize them?)

A successful response will contain authorization token.
It must be included in `Authorization` header using `Bearer` scheme for privileged endpoints.

```json
{
    "token": "(a token)"
}
```

---
`POST /auth/username`
Always accept the client.

Client will send its username in the request body.
It is trimmed before use.

This auth type is insecure and is mainly for debugging.

---

#### Privileged endpoints
Endpoints described here needs the authorization header,
or they will return 403 Forbidden.

---
`POST /channel/{ch}/stop`
Stops listening on the specified channel.

---
`GET /capture/desktop`
Get information about the available desktops.

Example:
```json
{
    "monitor": [
        {
            "id": "(opaque handle)",
            "name": "Generic PnP Monitor",
            "resolution": "1920x1080",
            /// Also can be specified as floating point number
            "refresh_rate": "60000/1001",
        }
    ]
}
```

---
`POST /capture/desktop`
Start streaming the desktop.

Example request:
```json
{
    "id": "(opaque handle)"
}
```

Example response:
```json
{
    /// The channel that the data is sent on.
    "ch": 1,
}
```

---
`GET /stream/v1?auth={token}`
Start WebSocket connection.

It upgrades the underlying connection into the WebSocket connection.
May return error if the version is unsupported.

Token is accepted via query string because of the browser limitation.


### WebSocket endpoint v1
(WIP)

Every message is `[u16le: stream id][bytes: flatbuffer data]` concatenated.
Stream ID 0 is control (`ControlFrame`).
Other channels are dynamically allocated.
