$mode=$args[0]
flyway -user="$env:POSTGRES_USER" -password="$env:POSTGRES_PASSWORD" -url="jdbc:postgresql://$env:POSTGRES_HOST/squadov"  -locations="filesystem:$PSScriptRoot/sql,filesystem:$PSScriptRoot/dev" -schemas="squadov" $mode