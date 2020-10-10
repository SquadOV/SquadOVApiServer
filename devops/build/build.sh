#!/bin/bash
set -xe
DIR=`dirname ${BASH_SOURCE[0]}`
ROOTDIR=`realpath ${DIR}/../../`

cd "${ROOTDIR}/config"
envsubst < config.toml.tmpl > ${GCP_PROJECT}.toml

cd "${ROOTDIR}"
TAG=registry.gitlab.com/squadov/squadovapiserver/${GCP_PROJECT}/squadov_api_server:latest
docker build . --tag ${TAG} --build-arg GCP_PROJECT=${GCP_PROJECT}
docker push ${TAG}