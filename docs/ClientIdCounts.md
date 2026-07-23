# ClientIdCounts

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**total** | **i32** | Total count across all clients (sum of per-client counts). | 
**client_ids** | **std::collections::HashMap<String, i32>** | Map of client ID to that client's reaction count. | 
**total_unidentified** | **i32** | Total count contributed by unidentified clients. | 
**clipped** | Option<**bool**> | Whether the `clientIds` map was truncated. | [optional]
**total_client_ids** | Option<**i32**> | Total number of distinct client IDs that reacted. | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


