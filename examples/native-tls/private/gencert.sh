#!/bin/bash

CA_SUBJECT="/C=JP/ST=Tokyo/O=Tsukuyomi CA/CN=Tsukuyomi Root CA"
SUBJECT="/C=JP/ST=Tokyo/O=Tsukuyomi/CN=localhost"

DIR="$(cd $(dirname $BASH_SOURCE); pwd)"
cd $DIR

set -ex

# generate RSA private key
openssl genrsa -out client.key 4096

# create Certificate Signing Request
openssl req -new \
  -subj "${CA_SUBJECT}" \
  -key client.key \
  -out client.csr

openssl x509 -req \
  -days 3650 \
  -signkey client.key \
  -in client.csr \
  -out client-ca.crt

openssl pkcs12 -export \
  -name "tsukuyomi" \
  -password "pass:mypass" \
  -inkey client.key \
  -in client-ca.crt \
  -out identity.p12
