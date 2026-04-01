package main

import (
	"fmt"
	"log"

	"github.com/hanwen/go-mtpfs/mtp"
)

func main() {
	fmt.Println("=== go-mtpfs MTP diagnostic ===")
	fmt.Println()

	dev, err := mtp.SelectDevice("")
	if err != nil {
		log.Fatalf("SelectDevice: %v", err)
	}
	defer dev.Done()

	dev.MTPDebug = true
	dev.USBDebug = true
	dev.DataDebug = true

	fmt.Println("\n--- Configure (Open + OpenSession with retry) ---")
	if err := dev.Configure(); err != nil {
		log.Fatalf("Configure: %v", err)
	}

	fmt.Println("\n--- SUCCESS: Session opened! ---")

	info := mtp.DeviceInfo{}
	if err := dev.GetDeviceInfo(&info); err != nil {
		log.Fatalf("GetDeviceInfo: %v", err)
	}
	fmt.Printf("  Manufacturer: %s\n", info.Manufacturer)
	fmt.Printf("  Model: %s\n", info.Model)
	fmt.Printf("  MTP Extension: %s\n", info.MTPExtension)

	sids := mtp.Uint32Array{}
	if err := dev.GetStorageIDs(&sids); err != nil {
		log.Fatalf("GetStorageIDs: %v", err)
	}
	fmt.Printf("  Storage IDs: %v\n", sids.Values)

	for _, sid := range sids.Values {
		si := mtp.StorageInfo{}
		if err := dev.GetStorageInfo(sid, &si); err != nil {
			fmt.Printf("  Storage 0x%x: error %v\n", sid, err)
			continue
		}
		fmt.Printf("  Storage 0x%x: %s, %d bytes free\n",
			sid, si.StorageDescription, si.FreeSpaceInBytes)
	}
}
