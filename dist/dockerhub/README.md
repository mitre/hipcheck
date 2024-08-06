# What is Hipcheck?

Hipcheck is a command line interface (CLI) tool for analyzing open source
software packages and source repositories to understand their software supply
chain risk. It analyzes a project's _software development practices_ and
detects _active supply chain attacks_ to give you both a long-term and
immediate picture of the risk from using a package.

## How to Use the Image

To run a short-lived container with the latest version of `hipcheck`, you might run:

NOTE: `latest` __always__ refers to the most-recently published image.

```
docker run mitre/hipcheck:latest
```

Hipcheck currently provides images for the following architectures:

* `linux/amd64`
* `linux/arm64`

## Helpful Links

* [Website](https://mitre.github.io/hipcheck)
* [Quickstart Guide](https://mitre.github.io/hipcheck/docs/quickstart/)
* [Complete Guide](https://mitre.github.io/hipcheck/docs/guide/)
* [Github](https://github.com/mitre/hipcheck)
* [Report an Issue](https://github.com/mitre/hipcheck/issues/new)

## License

Hipcheck's software is licensed under the Apache 2.0 license, which can be
found [here](https://github.com/mitre/hipcheck/blob/main/LICENSE)

