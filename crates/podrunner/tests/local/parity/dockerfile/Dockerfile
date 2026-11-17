# The image of the Dockerfile action of the parity suite (T-124).
# The mirror of Google: Docker Hub refuses an unnamed pull past its limit on a
# shared runner (runner-image 44100036389). The digest is the same.
FROM mirror.gcr.io/library/busybox@sha256:a7dbc319b1438115a207e11930dc4bb3e643dcb3ec0bdd694bb36cd8fad289ec
COPY entry.sh /entry.sh
ENTRYPOINT ["sh", "/entry.sh"]
