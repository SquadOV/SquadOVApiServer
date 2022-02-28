$url = "https://repo1.maven.org/maven2/org/flywaydb/flyway-commandline/8.5.0/flyway-commandline-8.5.0-windows-x64.zip"
$output = "flyway.zip"

Invoke-WebRequest -Uri $url -OutFile $output
Expand-Archive -Path $output -DestinationPath flyway_tmp
Move-Item -Path flyway_tmp/flyway-8.5.0 -Destination flyway
Remove-Item flyway_tmp -Recurse
Remove-Item $output