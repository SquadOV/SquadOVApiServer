#!/bin/bash
set -xe
DIR=`dirname ${BASH_SOURCE[0]}`
ROOTDIR=`realpath ${DIR}/../../`
COMMIT_HASH=`git rev-parse HEAD`

cd "${ROOTDIR}"
TAG=ff14_combat_log_builder:${COMMIT_HASH}
docker build . --file Dockerfile.ff14 --tag ${TAG}

DOCKER_ID=$(docker create ${TAG})
mkdir -p ${ROOTDIR}/devops/aws/lambda/build/ff14
docker cp ${DOCKER_ID}:/squadov/target/x86_64-unknown-linux-gnu/release/ff14_combat_log_parser ${ROOTDIR}/devops/aws/lambda/build/ff14/bootstrap
docker rm -v $DOCKER_ID

cd ${ROOTDIR}/devops/aws/lambda/build/ff14
zip ff14.zip bootstrap
mv ff14.zip ../../