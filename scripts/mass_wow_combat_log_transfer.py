import argparse
import csv
import subprocess
import json

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--csv', required=True)
    parser.add_argument('--host', required=True)
    parser.add_argument('--queue', required=True)
    parser.add_argument('--username', required=True)
    parser.add_argument('--password', required=True)
    parser.add_argument('--size', type=int, required=True)
    args = parser.parse_args()

    data = []
    with open(args.csv) as classes:
        reader = csv.DictReader(classes)
        for row in reader:
            if 'id' not in row:
                continue
            data.append(row['id'])

    subsets = []
    for i in range(0, len(data), args.size):
        subsets.append(data[i:i+args.size])

    def process(sub):
        cmd = [
            'rabbitmqadmin',
            '--host={}'.format(args.host),
            '--port=443',
            '--username={}'.format(args.username),
            '--password={}'.format(args.password),
            '--ssl',
            '--vhost={}'.format(args.username),
            'publish',
            'exchange=amq.default',
            'routing_key={}'.format(args.queue),
            'payload={}'.format(
                json.dumps(sub)
            )
        ]

        print(cmd)
        subprocess.call(cmd)

    for sub in subsets:
        process(sub)