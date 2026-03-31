# syntax=docker/dockerfile:1
FROM mysql:latest

ENV MYSQL_ROOT_PASSWORD my-secret-pw

COPY ./scripts/ /docker-entrypoint-initdb.d/
