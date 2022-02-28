#!/bin/bash
set -xe
DIR=`dirname ${BASH_SOURCE[0]}`
ROOTDIR=`realpath ${DIR}/../../`
COMMIT_HASH=`git rev-parse HEAD`

cd "${ROOTDIR}"
TAG=registry.gitlab.com/squadov/squadovapiserver/${DEPLOYMENT_ENVIRONMENT}/devapi:${COMMIT_HASH}
docker build . --tag ${TAG} --file Dockerfile.devapi
docker push ${TAG}