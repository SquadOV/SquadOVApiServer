import argparse
import os
import csv
import json

def extract_instances(dataFolder, outputFolder):
    allData = []

    with open(os.path.join(dataFolder, 'map.csv')) as classes:
        reader = csv.DictReader(classes)
        for row in reader:
            if row['InstanceType'] != '1' and row['InstanceType'] != '2' and row['InstanceType'] != '3' and row['InstanceType'] != '4' and row['InstanceType'] != '5':
                continue

            allData.append({
                'id': int(row['ID']),
                'name': row['MapName_lang'],
                'expansion': '',
                'parent': None,
            })

    with open(os.path.join(outputFolder, 'instances.json'), 'w') as f:
        json.dump(allData, f)

def extract_arenas(dataFolder, outputFolder):
    allData = []

    with open(os.path.join(dataFolder, 'map.csv')) as classes:
        reader = csv.DictReader(classes)
        for row in reader:
            if row['InstanceType'] != '4':
                continue
            allData.append({
                'id': int(row['ID']),
                'name': row['MapName_lang'],
                'expansion': '',
                'parent': None,
            })

    with open(os.path.join(outputFolder, 'arenas.json'), 'w') as f:
        json.dump(allData, f)

def extract_dungeons(dataFolder, outputFolder, expansions):
    allData = []

    with open(os.path.join(dataFolder, 'map.csv')) as classes:
        reader = csv.DictReader(classes)
        for row in reader:
            if row['InstanceType'] != '1':
                continue

            if int(row['ExpansionID']) not in expansions:
                continue

            allData.append({
                'id': int(row['ID']),
                'name': row['MapName_lang'],
                'expansion': expansions[int(row['ExpansionID'])],
                'parent': None,
            })

    with open(os.path.join(outputFolder, 'dungeons.json'), 'w') as f:
        json.dump(allData, f)

def extract_raids(dataFolder, outputFolder, expansions):
    raidData = []
    dataMap = {}

    with open(os.path.join(dataFolder, 'map.csv')) as classes:
        reader = csv.DictReader(classes)
        for row in reader:
            if row['InstanceType'] != '2':
                continue

            if int(row['ExpansionID']) not in expansions:
                continue

            data = {
                'id': int(row['ID']),
                'name': row['MapName_lang'],
                'expansion': expansions[int(row['ExpansionID'])],
                'parent': None,
            }
            raidData.append(data)
            dataMap[int(row['ID'])] = data

    with open(os.path.join(outputFolder, 'raids.json'), 'w') as f:
        json.dump(raidData, f)

    encounterData = []
    with open(os.path.join(dataFolder, 'dungeonencounter.csv')) as classes:
        reader = csv.DictReader(classes)
        for row in reader:
            pass
        
            mapId = int(row['MapID'])
            if mapId not in dataMap:
                continue

            encounterData.append({
                'id': int(row['ID']),
                'name': row['Name_lang'],
                'expansion': dataMap[mapId]['expansion'],
                'parent': mapId,
            })

    with open(os.path.join(outputFolder, 'encounters.json'), 'w') as f:
        json.dump(encounterData, f)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--data', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--vanilla', action='store_true')
    parser.add_argument('--tbc', action='store_true')
    args = parser.parse_args()

    if args.vanilla:
        expansions = {
            0: 'Vanilla',
        }
    elif args.tbc:
        expansions = {
            1: 'TBC',
        }
    else:
        expansions = {
            8: 'Shadowlands',
        }

    # Arenas (output everything)
    extract_arenas(args.data, args.output)

    # Dungeons (output relevant expansions)
    extract_dungeons(args.data, args.output, expansions)

    # Raids (output relevant expansions [instances + encounters])
    extract_raids(args.data, args.output, expansions)

    # Pure instance data
    extract_instances(args.data, args.output)

if __name__ == '__main__':
    main()