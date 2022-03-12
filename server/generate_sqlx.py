bins = [
    "squadov_api_server",
    "wow_kafka_worker",
    "vod_processing_worker",
    "singleton_event_processing_worker"
]

import subprocess
import os 
import json

finalData = None
for b in bins:

    args = [
        "cargo",
        "sqlx",
        "prepare",
        "--",
        "--bin",
        b
    ]

    if b == 'singleton_event_processing_worker':
        args.append('--features=eventloop')
    subprocess.call(args)

    dataFname = '{}.json'.format(b)
    os.replace('sqlx-data.json', dataFname)
    with open(dataFname, 'r') as f:
        data = json.load(f)
        if finalData is None:
            finalData = data
        else:
            for k, v in data.items():
                if k == 'db':
                    continue
                finalData[k] = v

with open('sqlx-data.json', 'w') as f:
    json.dump(finalData, f)