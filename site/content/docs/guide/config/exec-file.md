---
title: Execution Configuration
weight: 1
---

# Execution Configuration Files

As part of the Hipcheck plugin start-up process, there are specific values
that are hardcoded during the connection to the gRPC channel, such as the 
`backoff-interval` to indicate a wait time between connections. However, 
depending on system requirements, for example, these values may need to be 
updated.The Execution Configuration file is a [KDL](https://kdl.dev/) language 
configuration file that provides custom values for the plugin start-up process.

Here's what the default Execution Configuration file looks like:

```
plugin {
    backoff-interval 100000
    max-spawn-attempts 3
    max-conn-attempts 5
    jitter-percent 10
    grpc-msg-buffer-size 10
}
```

## The `plugin` Section

This section provides the variables required for the `PluginExecutor` module 
to make the gRPC connection. The configurable variables are as follows:

| Variable              | Default    | Description      |
| :-------------------- | :---------:| :--------------- |
| `backoff-interval`    | 100000     | (Microseconds) System wait time between connection attempts. |
| `max-spawn-attempts`  | 3          | Maximum number of attempts to spawn a gRPC channel. |
| `max-conn-attempts`   | 5          | Maximum number of attempts to make a plugin connection. |
| `jitter-percent`      | 10         | Percentage used with the `backoff-interval` and `max-conn-attempts` to calculate the sleep duration between connection attempts|
| `grpc-msg-buffer-size` | 10        | The size of the gRPC buffer |

## How It Works

The values in the Execution Configuration file are made available to 
Hipcheck through three ways:

### Default

Hipcheck can run just fine without explicitly making changes to any of these 
variables. The default values are hardcoded to be provided to the 
`PluginExecutor` by default.

### Exec.kdl in Hipcheck Binary

However, in the case that any of these values need to be updated, a sample
`Exec.kdl` file has been provided in `./config/`. You can use it by copying
the file to the root directory of Hipcheck. Any changes saved to the relocated 
`./Exec.kdl` will override the default Execution Configuration values, whether 
the file is stored at `hipcheck/Exec.kdl` or `.hipcheck/Exec.kdl`.

### Execution Configuration through CLI

If you would like to provide an Execution Configuration file stored elsewhere, 
you are able to use the optional CLI flag `-e <EXEC>`/`--exec <EXEC>` to enter
a valid file path.
