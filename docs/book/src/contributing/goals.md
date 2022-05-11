
# Goals

Hipcheck's goals are a statement of priorities to uphold during Hipcheck's
development. They are not hard and fast rules, but something to consider when
making choices about what features or changes to pursue, and how to pursue
them.

1. __Be able to handle large scale (1,000's of repositories)__
    - __Be fast__: Run as fast as possible. Benchmarking and profiling are key.
    - __Be judicious with memory__: Use as little memory as possible. OSS repositories can have a huge number of commits, and we should avoid duplication of that data wherever possible.
    - __Support numerous instances running in parallel__: Hipcheck works on single repositories, but will often want to be run against a collection of repositories. It should be easy to run Hipcheck in parallel, limiting the degree of contention on shared resources used by each instance.
2. __Be easy to "just run" and understand__
    - __Set good defaults__: Default values in the configuration should be tuned to a useful risk profile, so people can get valuable determinations out of the box. We should prefer to provide defaults when possible.
    - __Keep output clear__: Output should be clear and comprehensible for a user without requiring them to understand the deep details of how Hipcheck works.
    - __Keep the CLI simple__: Limit the use of feature flags. Prefer to set things in the config file. Use as few CLI arguments as possible.
    - __Provide useful errors__: Make sure errors are descriptive, suggest how to resolve problems, and provide a trace of what went wrong.
3. __Be easy to grow and develop__
    - __Keep compile times fast__: Hipcheck compiles relatively fast, and should continue to do so.
    - __Be well-commented__: Hipcheck's source code should include comments which explain intent, describe architecture, and note footguns, special cases, or exceptions.
