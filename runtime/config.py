import os

import yaml

class Config:
    def __init__(self):
        with open("config.yaml", 'r') as f:
            config = yaml.safe_load(f)
        if os.environ.get("RUN_ROUTE_CONFIG"):
            config["routeConfig"] = os.environ["RUN_ROUTE_CONFIG"]
        if os.environ.get("RUN_SPEED"):
            config["v"] = float(os.environ["RUN_SPEED"])
        for i in config:
            setattr(self, i, config[i])


config = Config()
