import argparse
from sqlalchemy import create_engine, Table, Column, Integer, String, MetaData, Boolean, ForeignKey
from sqlalchemy.sql import text
from sqlalchemy.dialects.postgresql import insert
import os
import json
import glob

metadata = MetaData()

wowRealms = Table('wow_connected_realms', metadata,
    Column('id', Integer, primary_key=True),
    Column('region', String, nullable=False),
    schema='squadov',
)

realmConnections = Table('wow_connected_realm_members', metadata,
    Column('connected_realm_id', Integer, nullable=False),
    Column('realm_id', Integer, nullable=False),
    schema='squadov',
)

def upsertRealms(stmt, idx, cols):
    update = dict()
    for c in cols:
        update[c] = stmt.excluded[c]

    return stmt.on_conflict_do_update(
        index_elements=idx,
        set_=update
    )

def upsertConnections(stmt, idx, cols):
    update = dict()
    for c in cols:
        update[c] = stmt.excluded[c]

    return stmt.on_conflict_do_update(
        index_elements=idx,
        set_=update
    )

regions = ['us', 'eu', 'kr', 'tw']

if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--realms', required=True)
    parser.add_argument('--jdbc', required=True)
    args = parser.parse_args()

    allRealms = []
    allConnections = []

    for reg in regions:
        files = glob.glob(os.path.join(args.realms, reg, '*.json'))
        for fname in files:
            with open(fname, encoding='utf-8') as f:
                data = json.load(f)

                allRealms.append({
                    'id': data["id"],
                    'region': reg
                })

                realms = data["realms"]
                for r in realms:
                    allConnections.append({
                        'connected_realm_id': data["id"],
                        'realm_id': r["id"],
                    })

    engine = create_engine(args.jdbc)
    metadata.create_all(engine)
    with engine.begin() as conn:
        conn.execute(upsertRealms(insert(wowRealms).values(allRealms), ['id'], ['id', 'region']))
        conn.execute(upsertConnections(insert(realmConnections).values(allConnections), ['connected_realm_id', 'realm_id'], ['connected_realm_id', 'realm_id']))