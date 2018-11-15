#!/bin/bash

CA_SUBJECT="/C=JP/ST=Tokyo/O=Tsukuyomi CA/CN=Tsukuyomi Root CA"
SUBJECT="/C=JP/ST=Tokyo/O=Tsukuyomi/CN=localhost"
ALT="DNS:localhost"

# generate RSA private key
openssl genrsa -out ca_key.pem 4096

# create Certificate Signing Request
openssl req -new -x509 \
  -days 3650 \
  -key ca_key.pem \
  -subj "${CA_SUBJECT}" \
  -out ca_cert.pem

openssl req -newkey rsa:4096 -nodes -sha256 \
  -keyout key.pem \
  -subj "${SUBJECT}" \
  -out server.csr

openssl x509 -req -sha256 \
  -extfile <(printf "subjectAltName=${ALT}") \
  -days 3650 \
  -CA ca_cert.pem \
  -CAkey ca_key.pem \
  -CAcreateserial \
  -in server.csr \
  -out cert.pem

rm ca_cert.srl server.csr
