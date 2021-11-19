import argparse
import json
import os
import shutil

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--json', required=True)
    parser.add_argument('--assets', required=True)
    parser.add_argument('--output', required=True)
    args = parser.parse_args()

    with open(args.json, 'r') as f:
        data = json.load(f)
    
    for d in data:
        unitName = d['display_name']
        championId = d['character_id']

        oFolder = os.path.join(args.output, championId)
        if not os.path.exists(oFolder):
            os.makedirs(oFolder)
        
        oJson = os.path.join(oFolder, 'data.json')
        with open(oJson, 'w') as f:
            json.dump({
                'name': unitName,
                'championId': championId,
                'cost': d['rarity'] + 1,
                'traits': list(map(lambda x: x['id'], d['traits']))
            }, f)

        iIcon = os.path.join(args.assets, d['squareIconPath'].replace('/lol-game-data/assets/ASSETS/', '').lower())
        oIcon = os.path.join(oFolder, 'icon.png')
        shutil.copy(iIcon, oIcon)