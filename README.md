# Enchanted Beans: got beans?

A memory-safe work queue backwards-compatible with beanstalkd.

## Features

* High compatibility with the original beanstalkd.
* High performance thanks to a modern, multi-threaded, async design.
* Assured memory safety thanks to Rust.

## Planned features

* Queue introspection: inspect jobs in the queue (not just the first in the queue).
* Queue management: change job priorities, or move them between states, based on the job content.
  * Supporting common data formats, including JSON and YAML, or plain old regex.
* Durable queues with a WAL and defined durability properties.
* Replication to another beanstalkd or super-beanstalkd server.
