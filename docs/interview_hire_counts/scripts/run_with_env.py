import io,os,sys,runpy
from pathlib import Path
from dotenv import dotenv_values
for k,v in dotenv_values(stream=io.StringIO((Path.home()/'Downloads/env').read_text().replace('\r',''))).items():
    if v is not None: os.environ.setdefault(k,v)
script=sys.argv[1]; sys.argv=sys.argv[1:]
sys.path.insert(0,str(Path(script).resolve().parent))
runpy.run_path(script,run_name='__main__')
