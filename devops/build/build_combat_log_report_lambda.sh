#!/bin/bash
set -xe
DIR=`dirname ${BASH_SOURCE[0]}`
ROOTDIR=`realpath ${DIR}/../../`
COMMIT_HASH=`git rev-parse HEAD`

cd "${ROOTDIR}"
TAG=combatlogreports:${COMMIT_HASH}
docker build . --file Dockerfile.reports --tag ${TAG}

DOCKER_ID=$(docker create ${TAG})
mkdir -p ${ROOTDIR}/devops/aws/lambda/build/reports
docker cp ${DOCKER_ID}:/squadov/target/x86_64-unknown-linux-gnu/release/combat_log_report_generator ${ROOTDIR}/devops/aws/lambda/build/reports/bootstrap
docker rm -v $DOCKER_ID

cd ${ROOTDIR}/devops/aws/lambda/build/reports
zip reports.zip bootstrap
mv reports.zip ../../