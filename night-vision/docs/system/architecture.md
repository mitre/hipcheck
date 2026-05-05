
# Architecture

The architecture of Night Vision is purposefully simple for the purposes of
our Minimum Viable Product (MVP).

In the future, assuming Night Vision achieves adoption, deployment, and scale,
it's very likely this architecture would need to evolve to meet growing needs;
but we've very purposefully decided not to prematurely design for scale that
may or may not happen.

!["Night Vision Architecture diagram, showing the frontend connecting to the
REST API, and the REST API connecting to the PostgreSQL database](./architecture.png)

<details>
<summary>The Mermaid text for the architecture diagram</summary>

```
architecture-beta
    group backend(cloud)[Backend]
    group frontend(cloud)[Frontend]

    service db(database)["PostgreSQL"] in backend
    service server(server)["REST API"] in backend
    service static(server)["Night Vision App"] in frontend

    db:L -- R:server
    static:R -- L:server
```

</details>
