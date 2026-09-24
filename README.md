
(Early development)
## Trellis
Tool for easily configuring and running Postgres servers.

### What is this for?
The goal is to make managing a Postgres server frictionless, reliable and repeatable.\
You should be able to write a config in a few minutes, track it in VCS and create an identical Postgres server instance in a single command.

### What can it do?
- You can declaratively configure a Postgres server with databases, schemas, roles, privileges, extensions etc.
- You can manage migrations and apply them when initialising your database or independently.
- You can spin up temporary servers that only maintain their data for the life of the process, perfect for tests.

### How to use?
Trellis is available as a binary to install through cargo using: `cargo install pg-trellis`

There's an example.trellis.yaml in the project root you can reference for creating your own configuration, run `trellis --help` to get a list of available args.

Proper documentation will follow as the project grows.
