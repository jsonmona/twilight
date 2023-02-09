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
There are two kinds of connection used in this project.
One is HTTP connection.
They are encrypted by TLS, and server does not care if it's
encrypted or not.
The other is WebSocket connection.
This is where the main communication is done.

The HTTP(s) and WebSocket transport trusts the outer TLS.
If either server or client reports that it's not encrypted,
An inner TLS session will be formed inside the WebSocket.
It happens when a) it sees unencrypted connection b) it sees
certificate that is both not in webpki and not Twilight Remote
Desktop certificate of the other one.

"Twilight Remote Desktop certificate" is
just a self-signed certificate that is used to advertise itself.
It's like SSH keys, except that both server and client has them.
They will remember fingerprint of that certificate to accomplish
"remember this PC" feature.

### HTTP Endpoints
Note: The default prefix for the HTTP endpoints is `/twilight`,
which is configurable. For example, `/twilight/auth`.

---
`POST /auth?type=cert`
Authorize the client using certificates.

Client will send its certificate in the request body.
Server will reply with its certificate.

Response status codes:  
200 &rarr; Successfully authorized. Client may proceed.  
400 &rarr; Not a valid request or signature.  
403 &rarr; Signature is successfully parsed, but client is not in
authorization list.

A successful response will set a cookie 🍪 to authenticate requests.

This endpoint is special.
Attempts to use other endpoints without the cookie will result in
a 404 Not Found response.

---
`GET /stream`
Start WebSocket connection.

It upgrades the underlying connection into the WebSocket connection.


### Flatbuffer protocol
WIP
