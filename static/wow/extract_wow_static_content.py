import argparse
import os
import csv
import json
import shutil

def load_listfile(fname):
    mapping = {}
    with open(fname, 'r') as f:
        for line in f:
            parts = line.split(';')
            if not parts[1].startswith('interface'):
                continue
            mapping[parts[0]] = parts[1].strip()
    return mapping

def extract_classes_to(data, interface, listfile, classOutput, specOutput, hasSpecs):
    if not os.path.exists(classOutput):
        os.makedirs(classOutput)

    if not os.path.exists(specOutput):
        os.makedirs(specOutput)

    allClasses = {}
    with open(os.path.join(data, 'chrclasses.csv')) as classes:
        reader = csv.DictReader(classes)
        for row in reader:
            allClasses[row['ID']] = {
                'id': row['ID'],
                'name': row['Name_lang'],
                'icon': listfile[row['IconFileDataID']],
                'specs': []
            }

    if hasSpecs:
        with open(os.path.join(data, 'chrspecialization.csv')) as specs:
            reader = csv.DictReader(specs)
            for row in reader:
                if row['ClassID'] not in allClasses:
                    continue

                allClasses[row['ClassID']]['specs'].append({
                    'id': row['ID'],
                    'name': row['Name_lang'],
                    'class': row['ClassID'],
                    'icon': listfile[row['SpellIconFileID']],
                })

    for classId, classData in allClasses.items():
        classFolder = os.path.join(classOutput, classId)
        if not os.path.exists(classFolder):
            os.makedirs(classFolder)

        classDataOutput = os.path.join(classFolder, 'data.json')

        classIconInput = os.path.join(interface, classData['icon'].replace('blp', 'png').replace('interface/', ''))
        classIconOutput = os.path.join(classFolder, 'icon.png')
        shutil.copy(classIconInput, classIconOutput)

        with open(classDataOutput, 'w') as f:
            json.dump(classData, f)

        for spec in classData['specs']:
            specFolder = os.path.join(specOutput, spec['id'])
            if not os.path.exists(specFolder):
                os.makedirs(specFolder)

            specDataOutput = os.path.join(specFolder, 'data.json')
            with open(specDataOutput, 'w') as f:
                json.dump(spec, f)

            specIconSrc = os.path.join(interface, spec['icon'].replace('blp', 'png').replace('interface/', ''))
            specIconDst = os.path.join(specFolder, 'icon.png')
            if not os.path.exists(specIconDst):
                shutil.copy(specIconSrc, specIconDst)

def extract_spells_to(data, interface, listfile, output):
    if not os.path.exists(output):
        os.makedirs(output)
    
    allSpells = {}
    with open(os.path.join(data, 'spellname.csv')) as spells:
        reader = csv.DictReader(spells)
        for row in reader:
            allSpells[row['ID']] = {
                'id': row['ID'],
                'name': row['Name_lang']
            }

    with open(os.path.join(data, 'spellmisc.csv')) as spells:
        reader = csv.DictReader(spells)
        for row in reader:
            if row['SpellID'] not in allSpells:
                continue

            if row['SpellIconFileDataID'] in listfile:
                allSpells[row['SpellID']]['icon'] = listfile[row['SpellIconFileDataID']]
            else:
                allSpells[row['SpellID']]['icon'] = None

    for spellId, spellData in allSpells.items():
        spellFolder = os.path.join(output, spellId)
        if not os.path.exists(spellFolder):
            os.makedirs(spellFolder)

        spellDataOutput = os.path.join(spellFolder, 'data.json')
        if not os.path.exists(spellDataOutput):
            with open(spellDataOutput, 'w') as f:
                json.dump(spellData, f)

        if 'icon' in spellData and spellData['icon'] is not None:
            spellIconInput = os.path.join(interface, spellData['icon'].replace('blp', 'png').replace('interface/', ''))
            if not os.path.exists(spellIconInput):
                continue

            spellIconOutput = os.path.join(spellFolder, 'icon.png')
            if not os.path.exists(spellIconOutput):
                shutil.copy(spellIconInput, spellIconOutput)

def extract_instances_to(data, interface, listfile, output):
    if not os.path.exists(output):
        os.makedirs(output)

    allInstances = {}
    with open(os.path.join(data, 'map.csv')) as maps:
        reader = csv.DictReader(maps)
        for row in reader:
            allInstances[row['ID']] = {
                'id': row['ID'],
                'name': row['MapName_lang'],
                'expansion': row['ExpansionID'],
                'loadingScreenId': row['LoadingScreenID'],
                'instanceType': row['InstanceType'],
            }

    allLoading = {}
    with open(os.path.join(data, 'loadingscreens.csv')) as loading:
        reader = csv.DictReader(loading)
        for row in reader:
            if 'MainImageFileDataID' in row and row['MainImageFileDataID'] != '0' and row['MainImageFileDataID'] in listfile:
                allLoading[row['ID']] = listfile[row['MainImageFileDataID']]
            elif row['WideScreen169FileDataID'] != '0' and row['WideScreen169FileDataID'] in listfile:
                allLoading[row['ID']] = listfile[row['WideScreen169FileDataID']]
            elif row['WideScreenFileDataID'] != '0' and row['WideScreenFileDataID'] in listfile:
                allLoading[row['ID']] = listfile[row['WideScreenFileDataID']]
            elif row['NarrowScreenFileDataID'] != '0' and row['NarrowScreenFileDataID'] in listfile:
                allLoading[row['ID']] = listfile[row['NarrowScreenFileDataID']]


    for instId, instanceData in allInstances.items():
        instFolder = os.path.join(output, instId)
        if not os.path.exists(instFolder):
            os.makedirs(instFolder)

        dataOutput = os.path.join(instFolder, 'data.json')
        if not os.path.exists(dataOutput):
            with open(dataOutput, 'w') as f:
                json.dump(instanceData, f)

        if instanceData['loadingScreenId'] in allLoading:
            src = os.path.join(interface, allLoading[instanceData['loadingScreenId']].replace('blp', 'png').replace('interface/', ''))
            if os.path.exists(src):
                dst = os.path.join(instFolder, 'background.png')

                if not os.path.exists(dst):
                    shutil.copy(src, dst)

def extract_difficulty_to(data, interface, output):
    if not os.path.exists(output):
        os.makedirs(output)

    allDiff = {}
    with open(os.path.join(data, 'difficulty.csv')) as difficulty:
        reader = csv.DictReader(difficulty)
        for row in reader:
            allDiff[row['ID']] = {
                'id': row['ID'],
                'name': row['Name_lang']
            }
    
    for diffId, diffData in allDiff.items():
        diffFolder = os.path.join(output, diffId)
        if not os.path.exists(diffFolder):
            os.makedirs(diffFolder)

        diffOutput = os.path.join(diffFolder, 'data.json')
        if not os.path.exists(diffOutput):
            with open(diffOutput, 'w') as f:
                json.dump(diffData, f)

def extract_items_to(data, interface, listfile, output, hasAppearance):
    if not os.path.exists(output):
        os.makedirs(output)

    allItems = {}

    with open(os.path.join(data, 'itemsparse.csv')) as items:
        reader = csv.DictReader(items)
        for row in reader:
            allItems[row['ID']] = {
                'id': int(row['ID']),
                'name': row['Display_lang'],
                'quality': int(row['OverallQualityID'])
            }

    with open(os.path.join(data, 'item.csv')) as items:
        reader = csv.DictReader(items)
        for row in reader:
            if not row['ID'] in allItems:
                continue

            allItems[row['ID']]['inventorySlot'] = int(row['InventoryType'])

            if row['IconFileDataID'] != '0' and row['IconFileDataID'] in listfile:
                allItems[row['ID']]['icon'] = listfile[row['IconFileDataID']]

    if hasAppearance:
        itemAppearance = {}
        with open(os.path.join(data, 'itemappearance.csv')) as items:
            reader = csv.DictReader(items)
            for row in reader:
                if row['DefaultIconFileDataID'] == '0' or row['DefaultIconFileDataID'] not in listfile:
                    continue

                itemAppearance[row['ID']] = {
                    'icon': listfile[row['DefaultIconFileDataID']]
                }

        with open(os.path.join(data, 'itemmodifiedappearance.csv')) as items:
            reader = csv.DictReader(items)
            for row in reader:
                if not row['ItemAppearanceID'] in itemAppearance:
                    continue

                if not row['ItemID'] in allItems:
                    continue
                allItems[row['ItemID']]['icon'] = itemAppearance[row['ItemAppearanceID']]['icon']

    for itemId, itemData in allItems.items():
        itemFolder = os.path.join(output, itemId)
        if not os.path.exists(itemFolder):
            os.makedirs(itemFolder)

        itemOutput = os.path.join(itemFolder, 'data.json')
        if not os.path.exists(itemOutput):
            with open(itemOutput, 'w') as f:
                json.dump(itemData, f)

        if 'icon' in itemData and itemData['icon'] is not None:
            itemIconInput = os.path.join(interface, itemData['icon'].replace('blp', 'png').replace('interface/', ''))
            if not os.path.exists(itemIconInput):
                continue

            itemIconOutput = os.path.join(itemFolder, 'icon.png')
            if not os.path.exists(itemIconOutput):
                shutil.copy(itemIconInput, itemIconOutput)

def extract_covenants_to(data, interface, output):
    if not os.path.exists(output):
        os.makedirs(output)

    allData = {}
    with open(os.path.join(data, 'covenant.csv')) as rawData:
        reader = csv.DictReader(rawData)
        for row in reader:
            allData[row['ID']] = {
                'id': int(row['ID']),
                'name': row['Name_lang']
            }
    
    for did, data in allData.items():
        dataFolder = os.path.join(output, did)
        if not os.path.exists(dataFolder):
            os.makedirs(dataFolder)

        dataOutput = os.path.join(dataFolder, 'data.json')
        if not os.path.exists(dataOutput):
            with open(dataOutput, 'w') as f:
                json.dump(data, f)

def extract_soulbinds_to(data, interface, output):
    if not os.path.exists(output):
        os.makedirs(output)

    allData = {}
    with open(os.path.join(data, 'soulbind.csv')) as rawData:
        reader = csv.DictReader(rawData)
        for row in reader:
            allData[row['ID']] = {
                'id': int(row['ID']),
                'name': row['Name_lang']
            }
    
    for did, data in allData.items():
        dataFolder = os.path.join(output, did)
        if not os.path.exists(dataFolder):
            os.makedirs(dataFolder)

        dataOutput = os.path.join(dataFolder, 'data.json')
        if not os.path.exists(dataOutput):
            with open(dataOutput, 'w') as f:
                json.dump(data, f)

def extract_conduits_to(data, interface, output):
    if not os.path.exists(output):
        os.makedirs(output)

    allData = {}
    with open(os.path.join(data, 'soulbindconduititem.csv')) as rawData:
        reader = csv.DictReader(rawData)
        for row in reader:
            allData[row['ConduitID']] = {
                'id': int(row['ConduitID']),
                'itemId': int(row['ItemID'])
            }
    
    for did, data in allData.items():
        dataFolder = os.path.join(output, did)
        if not os.path.exists(dataFolder):
            os.makedirs(dataFolder)

        dataOutput = os.path.join(dataFolder, 'data.json')
        if not os.path.exists(dataOutput):
            with open(dataOutput, 'w') as f:
                json.dump(data, f)

def extract_talents_to(data, interface, listfile, output):
    if not os.path.exists(output):
        os.makedirs(output)

    allData = {}
    with open(os.path.join(data, 'garrtalent.csv')) as rawData:
        reader = csv.DictReader(rawData)
        for row in reader:
            if not row['IconFileDataID'] in listfile:
                continue

            allData[row['ID']] = {
                'id': int(row['ID']),
                'name': row['Name_lang'],
                'socket': int(row['GarrTalentSocketPropertiesID']),
                'icon': listfile[row['IconFileDataID']]
            }

    for tid, data in allData.items():
        folder = os.path.join(output, tid)
        if not os.path.exists(folder):
            os.makedirs(folder)

        dataOutput = os.path.join(folder, 'data.json')
        if not os.path.exists(dataOutput):
            with open(dataOutput, 'w') as f:
                json.dump(data, f)

        if 'icon' in data and data['icon'] is not None:
            iconInput = os.path.join(interface, data['icon'].replace('blp', 'png').replace('interface/', ''))
            if not os.path.exists(iconInput):
                continue

            iconOutput = os.path.join(folder, 'icon.png')
            if not os.path.exists(iconOutput):
                shutil.copy(iconInput, iconOutput)

def extract_data_to(data, interface, output, vanilla, tbc):
    if not os.path.exists(output):
        os.makedirs(output)

    listfile = load_listfile(os.path.join(data, 'listfile.csv'))
    extract_difficulty_to(data, interface, os.path.join(output, 'difficulty'))
    extract_classes_to(data, interface, listfile, os.path.join(output, 'class'), os.path.join(output, 'specs'), not tbc and not vanilla)
    extract_instances_to(data, interface, listfile, os.path.join(output, 'instances'))
    extract_spells_to(data, interface, listfile, os.path.join(output, 'spells'))
    extract_items_to(data, interface, listfile, os.path.join(output, 'items'), tbc or not vanilla)

    if not vanilla and not tbc:
        extract_covenants_to(data, interface, os.path.join(output, 'covenants'))
        extract_soulbinds_to(data, interface, os.path.join(output, 'soulbinds'))
        extract_conduits_to(data, interface, os.path.join(output, 'conduits'))
        extract_talents_to(data, interface, listfile, os.path.join(output, 'talents'))

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--data', required=True)
    parser.add_argument('--interface', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--vanilla', action='store_true')
    parser.add_argument('--tbc', action='store_true')
    args = parser.parse_args()

    extract_data_to(args.data, args.interface, args.output, args.vanilla, args.tbc)

if __name__ == '__main__':
    main()