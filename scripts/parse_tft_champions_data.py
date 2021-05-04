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
        unitName = d['name']
        championId = d['championId']

        oFolder = os.path.join(args.output, championId)
        if not os.path.exists(oFolder):
            os.makedirs(oFolder)
        
        oJson = os.path.join(oFolder, 'data.json')
        with open(oJson, 'w') as f:
            json.dump(d, f)

        iIcon = os.path.join(args.assets, championId + '.png')
        if not os.path.exists(iIcon):
            iIcon = os.path.join(args.assets, unitName.replace(' ', '').replace("'", '').lower() + '.png')

        oIcon = os.path.join(oFolder, 'icon.png')
        shutil.copy(iIcon, oIcon)