Usage

Generate the certtificate:

```
$ ./private/gencert.sh
```

Test command:

```
$ curl --cacert ./private/ca_cert.pem https://localhost:4000/
```
