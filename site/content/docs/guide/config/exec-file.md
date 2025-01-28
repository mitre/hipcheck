---
title: Execution Configuration
weight: 1
---

# Execution Configuration Files

As part of Hipcheck's goal to make Hipcheck flexible for all users, the 
Execution Configuration file allows you to configure parameters of Hipcheck's 
execution in the case that the defaults are inappropriate for your system or 
use-case. The Execution Configuration file is a [KDL](https://kdl.dev/) 
configuration file that provides custom values used during Hipcheck's 
execution, including the plugin start-up process.

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

During the plugin start-up process, certain parameters are used for plugin
initialization and gRPC connection establishment. However, depending on 
system requirements, these values may need to be adjusted. The `plugin` section
contains the variables required for the `PluginExecutor` module to make the 
gRPC connection. The configurable variables are as follows:

| Variable              | Default    | Description      |
| :-------------------- | :---------:| :--------------- |
| `backoff-interval`    | 100000     | (Microseconds) System wait time between connection attempts. |
| `max-spawn-attempts`  | 3          | Maximum number of attempts to spawn a plugin subprocess. |
| `max-conn-attempts`   | 5          | Maximum number of attempts to establish a gRPC connection. |
| `jitter-percent`      | 10         | Percentage used with the `backoff-interval` and `max-conn-attempts` to calculate the sleep duration between connection attempts|
| `grpc-msg-buffer-size` | 10        | The size of the gRPC buffer |

## How It Works

The values in the Execution Configuration file are made available to Hipcheck
in three ways, listed in order of priority:

### Specifying on the CLI

To provide a custom Execution Configuration file for your Hipcheck execution,
there's a sample `Exec.kdl` file in `./config/`. You can use it by copying the 
file to your desired location, making and saving your changes while maintaining
KDL format. Use the optional CLI flag `-e <EXEC>`/`--exec <EXEC>` to specify the
file path for your Execution Configuration file.

### Automatic Exec.kdl Discovery

Alternatively, you can copy your custom Execution Configuration file
to the root directory of Hipcheck. When Hipcheck runs, it will first attempt to
read from `Exec.kdl` within the current working directory. If that's not 
available, it will search for `.hipcheck/Exec.kdl`. This search process starts 
in the current directory and continues recursively through all parent 
directories up to the root directory, similar to
[Cargo's configuration system](https://doc.rust-lang.org/cargo/reference/config.html).

### Default

In the case that an `Exec.kdl` file was not resolved using the above methods,
Hipcheck will use in-memory defaults for the values in this file.
