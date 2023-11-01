cd $PSScriptRoot
$files = Get-ChildItem -Path "." -Filter *.svg


foreach ($file in $files) {
	$name = $file.Name.Replace("svg", "png")
	# inkscape -b white -h 64 $file -o $name
	inkscape -h 256 $file -o $name
}