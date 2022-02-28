#!/bin/bash
OP=$1

flyway \
    -user="$REDSHIFT_USER" \
    -password="$REDSHIFT_PASSWORD" \
    -url="jdbc:redshift://$REDSHIFT_HOST:5439/squadov"  \
    -locations="filesystem:$PWD/redshift" \
    -driver="com.amazon.redshift.jdbc42.Driver" \
    -outOfOrder="true" \
    $OP