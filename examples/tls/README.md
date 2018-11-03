Usage

Generate the certificate and private key for testing:

```shell-session
$ cd private/
$ ./gencert.sh
```

Test command:

```shell-session
$ curl --cacert ./private/ca_cert.pem https://localhost:4000/
```
