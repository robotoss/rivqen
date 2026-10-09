# Transports

Rivqen always loads pages over HTTPS. Push channels are optional extras.

| Transport | Used for | Required? |
|---|---|---|
| HTTPS (HTTP/1.1, HTTP/2, HTTP/3) | Page loads, revalidation, data updates | **Yes** |
| WebSocket (WSS) | Push signals "this page changed" (RQP) | No |
| WebTransport | Experimental push channel (RQP) | No |

## Key points

1. The HTTP version (1.1, 2 or 3) is chosen by the platform network stack. You do not need to configure it.
2. A push channel only **tells** the client that a page changed. The client then checks the change. A push message never inserts content on its own authority.
3. If a push channel fails, Rivqen falls back to normal revalidation. Nothing breaks.
4. Push channels are off by default. They close when the app goes to the background.

::: info "Quick mode" is not QUIC
VasSonic has a client mode named "Quick". It has no relation to the QUIC network protocol used by HTTP/3.
:::

## Related

- [Engineering: Transport](/engineering/transport/)
