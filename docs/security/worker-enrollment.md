# Worker Enrollment and Certificate Management

This document describes the security architecture for communication between the TaskRun control plane and workers.

## Overview

### Goals

- **mTLS (mutual TLS)**: Both control plane and workers authenticate each other
- **Pinned CA**: Workers trust only the TaskRun CA, not the system trust store
- **Short-lived certificates**: Worker certs expire quickly, limiting blast radius
- **Secure enrollment**: Workers obtain certificates via a bootstrap token flow

### Non-Goals

- User/API authentication (separate concern, handled at API gateway level)
- Encryption at rest (handled by infrastructure)
- Network segmentation (handled by deployment)

---

## Certificate Authority

TaskRun uses an internal Certificate Authority (CA) for all control plane ↔ worker communication.

### CA Characteristics

| Property | Value |
|----------|-------|
| Key Algorithm | ECDSA P-256 or RSA 4096 |
| Validity | 10 years |
| Storage | Control plane only (file or HSM) |
| Trust Model | Workers trust ONLY this CA |

### Certificates Issued

1. **Server Certificate**: Used by control plane for TLS
   - CN: `taskrun-control-plane`
   - SANs: `localhost`, `[::1]`, configured hostnames
   - Validity: 1 year

2. **Worker Certificates**: Issued to each worker
   - CN: `worker:<worker_id>`
   - Validity: 7 days (short-lived)
   - Used for mTLS client authentication

---

## Bootstrap Token Flow

Before a worker can connect, it must obtain a certificate. This requires a bootstrap token.

### Token Generation

```
Admin                         Control Plane
  |                                 |
  |-- taskrun token generate ------>|
  |                                 |-- Generate 256-bit random token
  |                                 |-- Store SHA-256(token) with metadata
  |<---- bootstrap_token -----------|
  |                                 |
  |-- Deliver to worker (out-of-band)
```

### Token Properties

| Property | Value |
|----------|-------|
| Format | Base64-encoded 256-bit random |
| Validity | 1 hour (configurable) |
| Usage | Single-use (consumed on enrollment) |
| Storage | Control plane stores hash only |

### Token Delivery

The bootstrap token must be delivered to the worker out-of-band:

- **Development**: Environment variable or config file
- **Kubernetes**: Secret mounted as env var
- **Cloud**: Instance metadata or secrets manager

---

## Worker Enrollment Flow

```
Worker                                Control Plane
  |                                         |
  |-- Generate ECDSA P-256 keypair          |
  |-- Create CSR with worker_id             |
  |                                         |
  |-- POST /v1/enroll ---------------------->|
  |   {                                     |
  |     "bootstrap_token": "...",           |
  |     "csr": "-----BEGIN CERTIFICATE REQUEST-----..."
  |   }                                     |
  |                                         |
  |                                         |-- Validate bootstrap token
  |                                         |-- Check token not expired
  |                                         |-- Check token not already used
  |                                         |-- Mark token as consumed
  |                                         |-- Extract worker_id from CSR
  |                                         |-- Sign CSR with CA key
  |                                         |
  |<---- 200 OK ----------------------------|
  |   {                                     |
  |     "worker_cert": "-----BEGIN CERTIFICATE-----...",
  |     "ca_cert": "-----BEGIN CERTIFICATE-----...",
  |     "expires_at": "2025-12-14T00:00:00Z"
  |   }                                     |
  |                                         |
  |-- Store worker_cert, ca_cert locally    |
  |-- Connect to RunService with mTLS ----->|
```

### Enrollment Endpoint

```
POST /v1/enroll
Content-Type: application/json

{
  "bootstrap_token": "base64-encoded-token",
  "csr": "-----BEGIN CERTIFICATE REQUEST-----\n..."
}
```

### Response

```json
{
  "worker_cert": "-----BEGIN CERTIFICATE-----\n...",
  "ca_cert": "-----BEGIN CERTIFICATE-----\n...",
  "expires_at": "2025-12-14T00:00:00Z"
}
```

### Error Responses

| Code | Reason |
|------|--------|
| 400 | Invalid CSR format |
| 401 | Invalid or expired bootstrap token |
| 409 | Token already used |

---

## Certificate Lifecycle

### Validity Period

Worker certificates are intentionally short-lived:

| Environment | Validity | Renewal Threshold |
|-------------|----------|-------------------|
| Development | 7 days | 50% (3.5 days) |
| Production | 24 hours | 50% (12 hours) |

### Renewal Flow

```
Worker                                Control Plane
  |                                         |
  |-- Certificate at 50% lifetime           |
  |-- Generate new keypair                  |
  |-- Create CSR                            |
  |                                         |
  |-- POST /v1/renew ---------------------->|
  |   (mTLS with current cert)              |
  |   {                                     |
  |     "csr": "-----BEGIN CERTIFICATE REQUEST-----..."
  |   }                                     |
  |                                         |
  |                                         |-- Validate current client cert
  |                                         |-- Check cert not revoked
  |                                         |-- Sign new CSR
  |                                         |
  |<---- 200 OK ----------------------------|
  |   {                                     |
  |     "worker_cert": "...",               |
  |     "expires_at": "..."                 |
  |   }                                     |
  |                                         |
  |-- Swap to new cert atomically           |
```

### Revocation

The control plane maintains a revocation list for compromised workers:

```
POST /v1/workers/{worker_id}/revoke
```

Revoked workers:
- Cannot renew certificates
- Active connections are terminated
- worker_id is blocklisted

---

## mTLS Connection

### Handshake

```
Worker                                Control Plane
  |                                         |
  |-- TLS ClientHello -------------------->|
  |<---- TLS ServerHello + ServerCert -----|
  |                                         |
  |-- Verify ServerCert against pinned CA   |
  |                                         |
  |-- ClientCert (worker cert) ----------->|
  |                                         |
  |                                         |-- Verify ClientCert:
  |                                         |   - Signed by our CA
  |                                         |   - Not expired
  |                                         |   - Not revoked
  |                                         |   - Extract worker_id from CN
  |                                         |
  |<---- TLS Finished ---------------------|
  |                                         |
  |== Encrypted gRPC stream ===============|
```

### Worker ID Binding

The control plane extracts `worker_id` from the client certificate CN:

```
CN: worker:f538b94c-8217-4b6e-bfe5-f1a887c1b0f1
         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
         worker_id
```

This `worker_id` is bound to the gRPC stream, ensuring:
- Workers cannot impersonate other workers
- `WorkerHello.worker_id` must match certificate CN
- All operations are authenticated

---

## Key Storage

### Control Plane

| Asset | Location | Permissions |
|-------|----------|-------------|
| CA private key | `certs/ca.key` | 0600, root only |
| CA certificate | `certs/ca.crt` | 0644 |
| Server private key | `certs/server.key` | 0600 |
| Server certificate | `certs/server.crt` | 0644 |

### Worker

| Asset | Location | Permissions |
|-------|----------|-------------|
| Worker private key | `certs/worker.key` | 0600 |
| Worker certificate | `certs/worker.crt` | 0644 |
| CA certificate (pinned) | `certs/ca.crt` | 0644 |

---

## Configuration

### Control Plane

```toml
[tls]
# CA for signing worker certs
ca_cert = "certs/ca.crt"
ca_key = "certs/ca.key"

# Server certificate
server_cert = "certs/server.crt"
server_key = "certs/server.key"

# mTLS settings
require_client_cert = true
worker_cert_validity_days = 7

[enrollment]
bootstrap_token_validity_hours = 1
```

### Worker

```toml
[tls]
# Pinned CA (only trust this)
ca_cert = "certs/ca.crt"

# Worker certificate (after enrollment)
worker_cert = "certs/worker.crt"
worker_key = "certs/worker.key"

# Enrollment
bootstrap_token = "${TASKRUN_BOOTSTRAP_TOKEN}"
```

---

## Development Setup

For local development, generate self-signed certificates:

```bash
# Generate CA
openssl ecparam -genkey -name prime256v1 -out ca.key
openssl req -new -x509 -days 3650 -key ca.key -out ca.crt \
  -subj "/CN=TaskRun Development CA"

# Generate server cert
openssl ecparam -genkey -name prime256v1 -out server.key
openssl req -new -key server.key -out server.csr \
  -subj "/CN=taskrun-control-plane"
openssl x509 -req -days 365 -in server.csr -CA ca.crt -CAkey ca.key \
  -CAcreateserial -out server.crt \
  -extfile <(echo "subjectAltName=DNS:localhost,IP:127.0.0.1,IP:::1")

# Generate worker cert (for testing without enrollment)
openssl ecparam -genkey -name prime256v1 -out worker.key
openssl req -new -key worker.key -out worker.csr \
  -subj "/CN=worker:dev-worker-001"
openssl x509 -req -days 7 -in worker.csr -CA ca.crt -CAkey ca.key \
  -CAcreateserial -out worker.crt
```

---

## Security Considerations

### Threat Model

| Threat | Mitigation |
|--------|------------|
| Stolen bootstrap token | Single-use, short-lived (1h) |
| Stolen worker cert | Short validity (7 days), revocation support |
| Man-in-the-middle | mTLS, pinned CA |
| Replay attacks | TLS nonces, unique run_ids |
| Worker impersonation | worker_id bound to certificate CN |

### Recommendations

1. **Rotate CA** every 5 years (before 10-year expiry)
2. **Monitor** enrollment attempts and failures
3. **Alert** on certificate expiry approaching
4. **Audit** revocation list periodically
5. **HSM** for CA key in production

---

## Implementation Phases

1. **Phase 1: Basic TLS (#16)**
   - Self-signed certs for dev
   - TLS enabled on gRPC

2. **Phase 2: Internal CA (#17)**
   - Generate CA on first run
   - Sign server cert with CA
   - Workers pin to CA

3. **Phase 3: Enrollment (#19)**
   - `/enroll` endpoint
   - Bootstrap token flow
   - CSR signing

4. **Phase 4: mTLS (#20)**
   - Require client certs
   - Extract worker_id from cert
   - Bind to gRPC stream
