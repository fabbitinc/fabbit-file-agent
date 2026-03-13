!macro NSIS_HOOK_POSTINSTALL
  ; 자동 실행 등록
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "FabbitFileAgent" "$INSTDIR\${MAINBINARYNAME}.exe"

  ; 셸 폴더 등록 (탐색기 "내 PC" 하위)
  ; CLSID
  WriteRegStr HKCU "Software\Classes\CLSID\{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}" "" "Fabbit"
  WriteRegDWORD HKCU "Software\Classes\CLSID\{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}" "SortOrderIndex" 0x42
  WriteRegDWORD HKCU "Software\Classes\CLSID\{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}" "System.IsPinnedToNameSpaceTree" 1

  ; DefaultIcon
  WriteRegStr HKCU "Software\Classes\CLSID\{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}\DefaultIcon" "" "$INSTDIR\icons\icon.ico,0"

  ; InProcServer32
  WriteRegStr HKCU "Software\Classes\CLSID\{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}\InProcServer32" "" ""

  ; Instance
  WriteRegStr HKCU "Software\Classes\CLSID\{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}\Instance" "CLSID" "{0E5AAE11-A475-4c5b-AB00-C66DE400274E}"

  ; Instance\InitPropertyBag
  WriteRegDWORD HKCU "Software\Classes\CLSID\{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}\Instance\InitPropertyBag" "Attributes" 0x11
  WriteRegStr HKCU "Software\Classes\CLSID\{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}\Instance\InitPropertyBag" "TargetFolderPath" "$PROFILE\Fabbit"

  ; ShellFolder
  WriteRegDWORD HKCU "Software\Classes\CLSID\{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}\ShellFolder" "FolderValueFlags" 0x28
  WriteRegDWORD HKCU "Software\Classes\CLSID\{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}\ShellFolder" "Attributes" 0xF080004D

  ; "내 PC" NameSpace 등록
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\MyComputer\NameSpace\{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}" "" "Fabbit"

  ; 바탕화면 아이콘 숨김
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\HideDesktopIcons\NewStartPanel" "{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}" 1

  ; Fabbit 폴더 생성
  CreateDirectory "$PROFILE\Fabbit"

  ; 탐색기에 변경 알림
  System::Call "shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ; 셸 폴더 레지스트리 정리
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\MyComputer\NameSpace\{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}"
  DeleteRegKey HKCU "Software\Classes\CLSID\{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\HideDesktopIcons\NewStartPanel" "{E7B3A1D4-5F2C-4E89-9B6A-3D8F1C2E5A0B}"

  ; 탐색기에 변경 알림
  System::Call "shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)"
!macroend
