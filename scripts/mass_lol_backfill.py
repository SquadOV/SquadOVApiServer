import argparse
import json
import subprocess
import csv

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--data', required=True)
    parser.add_argument('--username', required=True)
    parser.add_argument('--password', required=True)
    args = parser.parse_args()

    with open(args.data) as data:
        reader = csv.DictReader(data)
        for row in reader:
            cmd = [
                'rabbitmqadmin',
                '--host=advanced-green-bird.rmq2.cloudamqp.com',
                '--port=443',
                '--username={}'.format(args.username),
                '--password={}'.format(args.password),
                '--ssl',
                '--vhost={}'.format(args.username),
                'publish',
                'exchange=amq.default',
                'routing_key=lol_api',
                'payload={}'.format(
                    json.dumps({
                        'type': 'LolMatch',
                        'platform': row['platform'],
                        'game_id': int(row['match_id']),
                    })
                )
            ]

            print(cmd)
            subprocess.call(cmd)