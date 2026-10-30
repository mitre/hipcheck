# Convenience image for manually running the exact Hipcheck artifacts embedded
# in Night Vision's backend image. The canonical artifact build is in
# ../Dockerfile; do not replace this with mitre/hipcheck:latest or a release tag.
ARG NV_SERVER_IMAGE=nv-server:hipcheck-mvp
FROM ${NV_SERVER_IMAGE}

ENTRYPOINT ["/usr/local/bin/hc"]
CMD ["--policy", "/opt/night-vision/hipcheck/config/Hipcheck.kdl", "--exec", "/opt/night-vision/hipcheck/config/Exec.kdl", "--cache", "/var/cache/night-vision/hipcheck", "ready"]
