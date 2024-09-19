# SPDX-License-Identifier: Apache-2.0

FROM node:bookworm-slim

ARG HC_VERSION="3.6.3"

WORKDIR /app

RUN set -eux \
    && apt-get update \
    && apt-get install -y git curl \
    && rm -rf /var/lib/apt/lists/* \
    && adduser --disabled-password hc_user \
    && chown -R hc_user /app \
    && curl --proto '=https' --tlsv1.2 -LsSf https://github.com/mitre/hipcheck/releases/download/hipcheck-v${HC_VERSION}/hipcheck-installer.sh | sh

USER hc_user
COPY config/ config/
ENV HC_CONFIG=./config
ENTRYPOINT ["./hc"]
CMD ["help"]
