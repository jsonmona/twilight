#ifndef TWILIGHT_COMMON_KEYPAIR_H
#define TWILIGHT_COMMON_KEYPAIR_H

#include "common/ByteBuffer.h"
#include "common/log.h"

#include <mbedtls/pk.h>

class Keypair {
public:
    Keypair();
    Keypair(const Keypair &copy) = delete;
    Keypair(Keypair &&move) = delete;
    ~Keypair();

    // Uses DER format
    void loadOrGenerate(const char *filename);

    bool parse(const ByteBuffer &key);
    bool parsePubkey(const ByteBuffer &pubkey);

    ByteBuffer privkey() const;
    ByteBuffer pubkey() const;

    ByteBuffer fingerprintSHA256() const;

    mbedtls_pk_context *pk() { return &ctx; }
    mbedtls_pk_context *operator->() { return pk(); }

private:
    static NamedLogger log;

    mbedtls_pk_context ctx;
};

#endif
