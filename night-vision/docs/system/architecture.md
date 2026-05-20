
# Architecture

This document describes the high-level architecture of Night Vision, meaning
how it's decomposed into separately-deployed components and how those
components interact.

## Table of Contents

[[_TOC_]]

## Goals

The architecture of Night Vision is purposefully simple for the purposes of
our Minimum Viable Product (MVP).

In the future, assuming Night Vision achieves adoption, deployment, and scale,
it's very likely this architecture would need to evolve to meet growing needs;
but we've very purposefully decided not to prematurely design for scale that
may or may not happen.

## Visualized

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

## Likely Future Changes

This is a rough collection of likely improvements we'll need to make in the
future, particularly if/when Night Vision begins to scale.

### Message Queues and Worker Tasks

A central part of Night Vision's design are two queues: the "re-analysis check
queue" and the "re-analysis queue." The re-analysis check queue is a queue for
tracked packages to be checked for potential updates which would require
re-analysis. The "re-analysis queue" is a queue for actually re-analyzing
packages which have been found to have relevant updates.

In the current design of Night Vision, both queues will be handled in-memory
within `nv-server`. While this is architecturally simple, it has obvious
limitations. The system will be constrained by the resources of a single
instance of `nv-server` (memory, IO bandwidth, etc.), and any scaling would be
done vertically by sizing up in the host on which `nv-server` is running,
rather than horizontally (by deploying more instances of `nv-server`). While
this works for a Minimum Viable Product, it does not work for a production
service at scale.

The obvious change would be split out these queues into an independent queue
system, using software such as RabbitMQ or Apache Kafka, and to introduce new
"workers" which can pull tasks off of these queues, and enqueue new tasks as
needed. In this architecture, scaling the system would mean standing up more
instances of the workers or addressing any bottlenecks arising in the message
queues themselves, which is more tractable than the solutions available today.

The tradeoff with such a system is complexity: complexity in deployment and
complexity in operation. In today's architecture, Night Vision has only three
"components" which need to be deployed and sustained in production: the
front-end application which serves the user interface, the backend server which
receives and responds to API requests, and the PostgreSQL database which
interacts with the backend server. With the introduction of separate queueing
infrastructure and worker tasks, we'd have multiple new "kinds" of things to
deploy. We'd need to solve more complex operational problems about how we
sustain a live application, and would likely reach for orchestration software
such as Kubernetes.

In the prior MIP effort on Night Vision, we pursued this kind of complexity
immediately, building around a microservice architecture from the start, and
trying to configure and deploy Kubernetes immediately. This was a mistake. We
had a small team, and were still actively defining and building the system
while we also attempted to wrangle a complex deployment story. In the end, the
burdens we took on from this approach were a key part of why we failed to
deliver a successful MVP by the end of the MIP period of performance. The
choice to delay this architectural change in the current Night Vision one is
purposeful, and based on lessons learned from that prior effort.

That said, it's almost certain that in the future, whether under the current
task or a subsequent task, we will need to decompose the Night Vision backend
monolith, split off workers to handle re-analysis checks and re-analysis
itself, and introduce distinct queueing infrastructure. We'll leave specific
choices about _how_ to handle that transition to when it is prudent to pursue.
