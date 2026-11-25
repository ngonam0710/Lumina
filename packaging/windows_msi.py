"""Build a per-user Windows Installer package from a Lumina release directory."""
import ctypes
import hashlib
import os
import subprocess
import sys
import tempfile
import uuid
from pathlib import Path

msi = ctypes.WinDLL("msi")
NS = uuid.UUID("1a49a968-f08b-48c9-b645-5a61539bbb62")


def guid(name):
    return "{" + str(uuid.uuid5(NS, name)).upper() + "}"


def check(result):
    if result:
        raise OSError(result, ctypes.FormatError(result))


def build(src, fonts, output, version):
    db = ctypes.c_uint()
    payload = [(src / "lumina.exe", "INSTALLDIR"), (src / "lumina-guard.exe", "INSTALLDIR")]
    payload += [(p, "FONTDIR") for p in sorted(fonts.iterdir()) if p.suffix in (".txt", ".ttf")]
    if not any(p.suffix == ".ttf" for p, _ in payload):
        raise ValueError("Missing bundled fonts")
    content = hashlib.sha256()
    for path, _ in payload:
        content.update(path.name.encode())
        content.update(path.read_bytes())
    product = guid(version + content.hexdigest())
    with tempfile.TemporaryDirectory(dir=output.parent) as td:
        tmp = Path(td)
        dest = tmp / "Lumina.msi"
        check(msi.MsiOpenDatabaseW(str(dest), ctypes.c_void_p(3), ctypes.byref(db)))

        def table(name, columns, types, keys, rows):
            lines = [columns.split(), types.split(), [name, *keys.split()], *rows]
            (tmp / (name + ".idt")).write_bytes(("\r\n".join("\t".join(map(str, row)) for row in lines) + "\r\n").encode("ascii"))
            check(msi.MsiDatabaseImportW(db, str(tmp), name + ".idt"))

        try:
            props = dict(ProductCode=product, ProductName="Lumina", Manufacturer="Lumina",
                         ProductLanguage="1033", ProductVersion=version, UpgradeCode=guid("upgrade"),
                         ARPNOMODIFY="1", SecureCustomProperties="OLDER;NEWER")
            table("Property", "Property Value", "s72 l0", "Property", props.items())
            table("Directory", "Directory Directory_Parent DefaultDir", "s72 S72 l255", "Directory", [
                ("TARGETDIR", "", "SourceDir"), ("LocalAppDataFolder","TARGETDIR","."),
                ("ProgramsDir","LocalAppDataFolder","Programs"), ("INSTALLDIR","ProgramsDir","Lumina"),
                ("FONTDIR","INSTALLDIR","fonts"), ("ProgramMenuFolder","TARGETDIR",".")])
            table("Feature","Feature Feature_Parent Title Description Display Level Directory_ Attributes",
                  "s38 S38 L64 L255 I2 i2 S72 i2","Feature",[("Main","","Lumina","",1,1,"INSTALLDIR",0)])
            components, files, features, registry = [], [], [], []
            directives = [".OPTION EXPLICIT", ".Set CabinetNameTemplate=payload.cab", ".Set DiskDirectoryTemplate=.",
                          ".Set Cabinet=on", ".Set Compress=on", ".Set CompressionType=MSZIP", ".Set MaxDiskSize=0"]
            for i, (p, folder) in enumerate(payload, 1):
                ident = "F" + str(i)
                component = "C" + str(i)
                key = "K" + str(i)
                components.append((component,guid("install/"+p.name),folder,4,"",key))
                features.append(("Main",component))
                registry.append((key,1,r"Software\Lumina\Installer",p.name,"1",component))
                short = "L" + str(i) + p.suffix.upper()
                files.append((ident, component, short+"|"+p.name,p.stat().st_size,"","",16384,i))
                directives.append('"'+str(p)+'" '+ident)
            (tmp/"files.ddf").write_text("\n".join(directives), encoding="mbcs")
            subprocess.run(["makecab","/F","files.ddf"],cwd=tmp,check=True,stdout=subprocess.DEVNULL)
            table("Component","Component ComponentId Directory_ Attributes Condition KeyPath","s72 S38 s72 i2 S255 S72","Component",components)
            table("FeatureComponents","Feature_ Component_","s38 s72","Feature_ Component_",features)
            table("Registry","Registry Root Key Name Value Component_","s72 i2 l255 L255 L0 s72","Registry",registry)
            table("File","File Component_ FileName FileSize Version Language Attributes Sequence","s72 s72 l255 i4 S72 S20 I2 i2","File",files)
            table("Media","DiskId LastSequence DiskPrompt Cabinet VolumeLabel Source","i2 i2 L64 S255 S32 S72","DiskId",[(1,len(files),"","#payload.cab","","")])
            table("Shortcut","Shortcut Directory_ Name Component_ Target Arguments Description Hotkey Icon_ IconIndex ShowCmd WkDir",
                  "s72 s72 l128 s72 s72 S255 L255 I2 S72 I2 I2 S72","Shortcut",
                  [("LuminaLink","ProgramMenuFolder","Lumina","C1","[#F1]","--open-settings","Lumina","","","",1,"INSTALLDIR")])
            table("Upgrade","UpgradeCode VersionMin VersionMax Language Attributes Remove ActionProperty","s38 S20 S20 S255 i4 S255 s72",
                  "UpgradeCode VersionMin VersionMax Language Attributes",[(guid("upgrade"),"",version,"",512,"","OLDER"),(guid("upgrade"),version,"","",2,"","NEWER")])
            table("LaunchCondition","Condition Description","s255 l255","Condition",[("NOT NEWER","A newer Lumina version is installed.")])
            actions = ["FindRelatedProducts","LaunchConditions","CostInitialize","FileCost","CostFinalize","InstallValidate","InstallInitialize",
                       "RemoveExistingProducts","ProcessComponents","UnpublishFeatures","RemoveShortcuts","RemoveRegistryValues","RemoveFiles","InstallFiles",
                       "WriteRegistryValues","CreateShortcuts","RegisterUser","RegisterProduct","PublishFeatures","PublishProduct","InstallFinalize"]
            table("InstallExecuteSequence","Action Condition Sequence","s72 S255 I2","Action",[(a,"",100*n) for n,a in enumerate(actions,1)])
            admin = ["CostInitialize","FileCost","CostFinalize","InstallValidate","InstallInitialize","InstallAdminPackage","InstallFiles","InstallFinalize"]
            table("AdminExecuteSequence","Action Condition Sequence","s72 S255 I2","Action",[(a,"",100*n) for n,a in enumerate(admin,1)])
            view = ctypes.c_uint()
            record = msi.MsiCreateRecord(1)
            try:
                check(msi.MsiDatabaseOpenViewW(db,"INSERT INTO \x60_Streams\x60 (\x60Name\x60, \x60Data\x60) VALUES ('payload.cab', ?)",ctypes.byref(view)))
                check(msi.MsiRecordSetStreamW(record,1,str(tmp/"payload.cab")))
                check(msi.MsiViewExecute(view,record))
            finally:
                msi.MsiCloseHandle(view)
                msi.MsiCloseHandle(record)
            info = ctypes.c_uint()
            check(msi.MsiGetSummaryInformationW(db,None,7,ctypes.byref(info)))
            try:
                for prop, val in [(2,"Installation Database"),(3,"Lumina"),(4,"Lumina"),(7,"x64;1033"),(9,guid("package"+product))]:
                    check(msi.MsiSummaryInfoSetPropertyW(info,prop,30,0,None,val))
                for prop, val in [(14,200),(15,10)]:
                    check(msi.MsiSummaryInfoSetPropertyW(info,prop,3,val,None,None))
                check(msi.MsiSummaryInfoPersist(info))
            finally:
                msi.MsiCloseHandle(info)
            check(msi.MsiDatabaseCommit(db))
        finally:
            msi.MsiCloseHandle(db)
        os.replace(dest, output)


if __name__ == "__main__":
    release, font_dir, target, version = sys.argv[1:]
    build(Path(release).resolve(), Path(font_dir).resolve(), Path(target).resolve(), version)
