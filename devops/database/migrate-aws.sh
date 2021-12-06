#!/bin/bash
OP=$1

flyway \
    -user="$POSTGRES_USER" \
    -password="$POSTGRES_PASSWORD" \
    -url="jdbc:postgresql://$DATABASE_HOST/squadov"  \
    -locations="filesystem:$PWD/sql,filesystem:$PWD/prod" \
    -schemas="squadov" \
    -outOfOrder="true" \
    $OP