## how to run tests
### run all tests:
this is for logs
```bash
cargo test -- --test-threads=1 --nocapture
```

without logs:
```bash
cargo test -- --test-threads=1
```

### run an individual test:
logs:
```bash
cargo test <test_name> --nocapture
```

no logs:
```bash
cargo test <test_name>
```

> Note: For better readability, pipe the cargo test command to a file, the logs can get large.