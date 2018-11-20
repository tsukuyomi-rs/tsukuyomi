Start server:

```shell-session
$ cargo run /path/to/socket.sock
```

Test command:

```
$ curl --unix-socket /path/to/socket.sock http://localhost/"
```
